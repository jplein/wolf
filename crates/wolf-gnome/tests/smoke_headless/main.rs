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
//! ## What this binary contains
//!
//! This file holds the shared harness: cage launch and [`CageSlots`], readiness, the
//! temp, state, config and shader-cache directories, screenshots and capture decoding,
//! the OCR helpers, Neovim readbacks, the pointer-injection harness and the shared
//! geometry. Each feature's fixtures, scenarios, helpers and smokes live in a module of
//! their own, declared below, and each smoke's own doc states its evidence.
//!
//! Every `*_under_cage` test is a cage smoke: it launches the real binary under `cage`,
//! and it runs in the standard suite. Every other test is a compositor-free guard, which
//! mechanically enforces an invariant the cage smokes depend on but cannot check
//! themselves: none calls `require_smoke_env`, takes a global cage slot or spawns a
//! subprocess, so they still guard on a machine where the compositor is unavailable.
//!
//! The guards that need something from the environment are named here, and the claim is
//! stated precisely rather than loosely. [`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`]
//! measures a real cell through [`cell_metrics_for_smoke`], so it needs the devShell's
//! pinned `FONTCONFIG_FILE`. That is not part of the cage toolchain listed below — it is
//! the same font pinning `tests/paint_golden.rs` needs — but it is not nothing, and a run
//! outside the devShell fails it for that reason rather than for the reason its message
//! names. [`every_cage_gets_a_private_runtime_dir_and_no_host_session`] builds real
//! [`cage_command`]s but looks nothing up on `PATH`: it needs only a writable
//! `temp_dir()`, where each command writes its bus configuration.
//!
//! So this target's summary line counts both kinds as `passed`. The `flake.nix` guards
//! are a binary of their own, `tests/flake_exports.rs`, because they read only
//! `flake.nix` and need none of this harness.
//!
//! The `#[ignore]`d PACKAGED smokes of the `packaged` module are NOT in that count. They
//! drive `$out/bin/wolf-gnome` — the artifact `nix build .#wolf-gnome` installs — rather
//! than `CARGO_BIN_EXE_wolf-gnome`, so they need a store path handed to them in
//! `WOLF_PACKAGE_OUT` and exactly one command supplies it: `./tools/wolf-package-smoke.sh`.
//! An ignored test is not part of the default suite, which is a different thing from a
//! silently-skipped one — their precondition still fails loud. They live in this binary
//! because they launch through [`cage_command`] and hold a [`CAGE_SLOTS`] slot like every
//! other smoke.
//!
//! ## What the smokes read
//!
//! Most smokes OCR a screenshot of the live window, and some OCR a crop of it, where
//! which part of the window draws a term is the claim. A smoke whose claim is a colour or
//! a position decodes its own captures and reads raw pixels, which is the only route this
//! harness has to a claim OCR cannot make: it reads no colour, and it cannot answer
//! "where on the page is this". A smoke whose claim no capture could carry asserts on
//! readbacks the captive Neovim writes about its own state, on bytes on disk, on a frozen
//! `state.json` or on its exit status alone.
//!
//! A smoke that needs two app launches — a relaunch, two configured values, two windows —
//! runs **both inside ONE `cage` instance** (one scenario string, two sequential
//! `"$WOLF_BIN" &` blocks), so it still holds one cage slot and starts one compositor,
//! and the second launch starts only after the first has exited. See
//! `restore::two_windows_two_neovims_and_two_saved_sessions_under_cage` for why a second
//! overlapping process is not an alternative.
//!
//! The pointer smokes additionally run **`wolf-pointer-inject`** inside cage — this repo's
//! own long-lived `zwlr_virtual_pointer_v1` client, see
//! `tools/wolf-pointer-inject/README.md` — started BEFORE the app so the seat already
//! carries `WL_SEAT_CAPABILITY_POINTER` when GTK does its first `wl_seat` roundtrip, and
//! driven over a FIFO with a poll-for-ack protocol. Most of them assert on files the
//! captive Neovim writes about its own state: exact values that no sidebar row and no
//! `:e` toast can satisfy, and that OCR is structurally incapable of answering, as in "did
//! the cursor land at exactly row 12, column 25".
//!
//! ## Readiness before the first input
//!
//! No scenario sends a key, a pointer event or a capture before `wait_ready` returns.
//! [`cage_command`] sets `WOLF_READY_FILE` and prepends [`READINESS_PRELUDE`], whose
//! `wait_ready` polls until the app has written its own pid there — which it does
//! once its launch windows accept keystrokes (`notes/wolf-gnome-decisions.md`, "The
//! readiness file"). A launch that never gets there exits 40 after 30 s, and the
//! app's `wolf-ready: waiting on <condition>` stderr lines name the conjunct it was
//! stuck on. No fixed sleep stands in for this, because launch latency grows with
//! load. Before polling, `wait_ready` primes the seat with a key-free `wtype ""`:
//! a headless `cage` seat has no keyboard until a virtual keyboard has existed, and
//! without that the first key of the first keyboard is lost.
//!
//! ## When it runs
//!
//! The cage smokes run as part of the STANDARD suite — `cargo test -p
//! wolf-gnome` and `cargo test --workspace` — inside the flake devShell. They are
//! the Linux verification gate's only end-to-end proof that the GTK front-end draws
//! a window a human could see (see `build.md`, "The verification gate").
//!
//! They are deliberately NOT `#[ignore]`d. "They are slow" is not a reason to skip
//! the only real end-to-end test the front-end has; per `CLAUDE.md` a test the
//! suite command does not run is a silently-skipped test.
//!
//! They are, however, the slowest part of the suite: each drives a real
//! compositor. They run [`smoke_jobs`] at a time, each in a [`CageSlots`] slot and a
//! private runtime directory; `WOLF_SMOKE_JOBS=<n>` overrides the count, and `1` runs
//! them one at a time.
//!
//! ```text
//! nix develop --command cargo test -p wolf-gnome --test smoke_headless
//! ```
//!
//! A single smoke is selected by its module path, as in
//! `-- --exact editor::window_renders_typed_text_under_cage`.
//!
//! ## Requirements of the cage smokes (from the devShell's smoke toolchain)
//!
//! - `cage` (headless wlroots compositor), `dbus-run-session`, `wtype`, `grim`,
//!   `tesseract` and `wolf-pointer-inject` on `PATH`. The last one is NOT an upstream package: it is
//!   built from this repo's own `tools/wolf-pointer-inject/` by `flake.nix`, so
//!   `command -v wolf-pointer-inject` succeeding can only mean the flake provided
//!   it. Only the pointer-driven smokes use it, but it is required
//!   unconditionally — see [`REQUIRED_TOOLS`].
//! - No D-Bus configuration from the host or the store: [`smoke_bus_config`] writes
//!   each cage's own into its runtime directory.
//! - `WOLF_NVIM_BIN` pointing at the pinned Neovim (same contract as the other
//!   wolf-gnome tests; nvim is never taken from `PATH`).
//! - `WOLF_RG_BIN` pointing at the pinned ripgrep. `WolfCore::start_search` resolves
//!   it from that variable ALONE and never from `PATH`, so an unset one gives the
//!   project-search panel an immediately-complete session with zero matches. See
//!   [`require_smoke_env`], which fails loud rather than letting that surface as an
//!   empty result list.
//!
//! Per project policy every cage smoke FAILS LOUD if any of the above is
//! missing rather than silently skipping, mirroring `common::require_nvim_bin` — a
//! missing precondition means the smoke toolchain was not provisioned, which is a
//! setup bug we want surfaced, not hidden. Now that these tests run by default, that
//! hard fail matters more, not less: a missing tool means the suite was invoked
//! outside the devShell, and softening `require_smoke_env` to a skip would let the
//! gate pass green on an unprovisioned machine while proving nothing. Do not.
//!
//! `preview::preview_created_after_launch_renders_without_a_portal_under_cage` also
//! runs `dbus-send`, which the devShell's `pkgs.dbus` installs beside
//! `dbus-run-session`, to ask its bus what it can activate. It is not in
//! [`REQUIRED_TOOLS`], because no other smoke runs it; its absence fails that smoke,
//! loudly, with exit 17.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use cairo::{Context, Format, ImageSurface};
use wolf_core::config::CONFIG_FILE_NAME;
use wolf_core::state::STATE_FILE_NAME;
use wolf_core::{
    load_state, preferences_dir, state_dir, CommandMru, PaneNode, PaneOrientation, RecentDirs,
};
use wolf_gnome::{preview_coalescer::PREVIEW_TRAILING_DELAY, CellMetrics, WINDOW_TITLE_FALLBACK};

mod compose;
mod config;
mod editor;
mod fixture_guards;
mod frame_stats;
mod packaged;
mod panes;
mod pointer;
mod preview;
mod preview_render;
mod recovery;
mod restore;
mod scrub_lists;
mod sidebar;
mod terminal;

use scrub_lists::{PACKAGED_SCRUB_ALWAYS, PACKAGED_SCRUB_FONT_STACK, PACKAGED_SCRUB_GTK_RUNTIME};

/// The environment variable that sets how many cage smokes run at once. `1` runs
/// them one at a time; unset, [`parse_smoke_jobs`] derives the default.
const SMOKE_JOBS_ENV: &str = "WOLF_SMOKE_JOBS";

/// Host threads per cage in the default job count: unset, [`SMOKE_JOBS_ENV`] is
/// `available_parallelism / DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE`, clamped to
/// `1..=`[`DEFAULT_SMOKE_JOBS_CAP`].
///
/// Each cage runs a compositor, a GTK app with WebKit, a captive Neovim and, after
/// the scenario, `tesseract`, and every scenario waits on fixed settles that load
/// stretches. The per-cage share and the cap come from measurement; the table is in
/// [`DEFAULT_SMOKE_JOBS_CAP`]'s doc.
const DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE: usize = 2;

/// The most cages the default runs at once, whatever the host.
///
/// Measured on the 32-thread development host (AMD Ryzen AI MAX+ 395), full
/// `smoke_headless` runs at each `WOLF_SMOKE_JOBS`, every one `73 passed`:
///
/// | jobs | `finished in` | process user time |
/// | --- | --- | --- |
/// | 1 | 543.2 s | 73.4 s |
/// | 2 | 280.3 s | 67.8 s |
/// | 4 | 137.4 s | 64.9 s |
/// | 8 | 77.1 s, 75.1 s | 65.6 s, 65.1 s |
/// | 12 | 76.0 s, 70.5 s | 65.2 s, 66.1 s |
/// | 16 | 65.2 s, 64.2 s | 66.5 s, 65.4 s |
///
/// The default is the smallest count whose mean of two runs is within about 10 % of
/// the best mean, with no failure: 16 (64.7 s against 73.3 s at 12), so the pair
/// gives 16 on that host. Past 8 the wall time is
/// bounded by the slowest smokes rather than by the slots. The user time is the
/// harness's own reaped children and undercounts the killed apps, so it shows only
/// that the work does not grow with concurrency.
const DEFAULT_SMOKE_JOBS_CAP: usize = 16;

/// The cage-slot count for a [`SMOKE_JOBS_ENV`] value `raw` (`None` when unset) on
/// a host with `parallelism` threads.
///
/// An explicit value is trimmed and must be at least 1. It is deliberately not
/// capped: the cap shapes the default only, and a stress run needs to go past it.
/// `0` is an error rather than "the default", because zero slots would block every
/// smoke in [`CageSlots::acquire`] with nothing reported.
///
/// **Gotcha:** the lack of a cap has a practical ceiling. Each running cage takes one
/// X display number for its Xwayland, and wlroots tries only about 33 of them, so a
/// value much above about 30 can leave a cage's Xwayland with no display to start
/// on. 24 is the largest value measured green.
fn parse_smoke_jobs(raw: Option<&str>, parallelism: usize) -> Result<usize, String> {
    let Some(raw) = raw else {
        return Ok(
            (parallelism / DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE).clamp(1, DEFAULT_SMOKE_JOBS_CAP)
        );
    };
    match raw.trim().parse::<usize>() {
        Ok(jobs) if jobs >= 1 => Ok(jobs),
        Ok(_) => Err(format!(
            "{SMOKE_JOBS_ENV}={raw:?} admits no cage at all; it must be at least 1 (1 runs the \
             cage smokes one at a time)"
        )),
        Err(e) => Err(format!(
            "{SMOKE_JOBS_ENV}={raw:?} is not a whole number of cage slots ({e}); unset it for the \
             default, or set 1 to run the cage smokes one at a time"
        )),
    }
}

/// This process's cage-slot count: [`SMOKE_JOBS_ENV`] through [`parse_smoke_jobs`],
/// read once. Panics with the parse error when the variable is set but unusable.
fn smoke_jobs() -> usize {
    static JOBS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *JOBS.get_or_init(|| {
        let raw = match std::env::var(SMOKE_JOBS_ENV) {
            Ok(raw) => Some(raw),
            Err(std::env::VarError::NotPresent) => None,
            Err(e) => panic!("{SMOKE_JOBS_ENV} is unreadable: {e}"),
        };
        let parallelism = std::thread::available_parallelism().map_or(1, usize::from);
        parse_smoke_jobs(raw.as_deref(), parallelism).unwrap_or_else(|e| panic!("{e}"))
    })
}

/// A counting semaphore over the cage smokes.
///
/// **Decisions**
///
/// - It is per process. Two `cargo test` invocations each size their slots for the
///   whole host, so running them at once oversubscribes it; nothing here can bound
///   across them, which is why the docs still say not to overlap runs.
/// - A smoke holds its slot for its whole body, OCR included, because `tesseract`
///   competes for the same cores as the next cage's fixed settles.
/// - Concurrency is `min(`[`SMOKE_JOBS_ENV`]`, --test-threads)`: libtest's own
///   thread pool is the outer bound, and a slot only throttles below it.
/// - A poisoned mutex is recovered with `into_inner`, so one panicking smoke does
///   not fail the rest; the counter is only ever changed under the lock.
struct CageSlots {
    /// Holders inside.
    running: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
}

impl CageSlots {
    const fn new() -> Self {
        Self {
            running: std::sync::Mutex::new(0),
            freed: std::sync::Condvar::new(),
        }
    }

    /// Blocks until fewer than `limit` slots are held, and holds one until the
    /// returned guard drops.
    fn acquire(&self, limit: usize) -> CageSlotGuard<'_> {
        let running = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut running = self
            .freed
            .wait_while(running, |running| *running >= limit)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *running += 1;
        CageSlotGuard { slots: self }
    }
}

/// One held slot of a [`CageSlots`]; dropping it frees the slot and wakes every
/// waiter.
struct CageSlotGuard<'a> {
    slots: &'a CageSlots,
}

impl Drop for CageSlotGuard<'_> {
    fn drop(&mut self) {
        let mut running = self
            .slots
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *running -= 1;
        drop(running);
        self.slots.freed.notify_all();
    }
}

/// The cage slots every smoke in this process takes through [`require_smoke_env`].
static CAGE_SLOTS: CageSlots = CageSlots::new();

/// Font size (points) handed to the app via `WOLF_FONT_PT`. Larger than the
/// interactive default so the rendered glyphs are big enough for `tesseract` to
/// recognize reliably in a screenshot.
const SMOKE_FONT_PT: &str = "28";

/// The `appearance` every smoke's seeded `wolf-config.yaml` carries — see
/// [`write_smoke_config`], whose doc comment holds the reason the key is PINNED at
/// all and the reason it is pinned to this value in particular.
///
/// Only the two-launch appearance smoke passes anything else, and it passes both
/// of the other values in one run.
const SMOKE_APPEARANCE: &str = "dark";

/// The `line-height` (percent of the font size) every smoke's seeded
/// `wolf-config.yaml` carries — see [`smoke_config_dir`], which writes it, and
/// [`smoke_line_height`], which parses it back for this binary's own measurement.
///
/// ## Why this exact number
///
/// It is the value at which the configured cell height reproduces the
/// pre-config one EXACTLY, so no derived pixel target in this binary moves.
/// Measured on this host with the pinned DejaVu Sans Mono at
/// [`SMOKE_FONT_PT`]: the Pango context over a 1x1 `ImageSurface` reports no
/// resolution, so `CellMetrics::for_font` takes its 96 dpi arm and
/// `size_px = 28 * 96 / 72 = 37.3333`; `ascent` is 35, `descent` is 9, and
/// `metrics.height()` is 44, so the clamped `leading` is exactly 0. The cell
/// height the app computes is therefore `round(37.3333 * 1.18) = round(44.05) =
/// 44`, which is `ascent + descent` — byte-identical to the height the app used
/// before it read a config at all.
///
/// Two consequences, both load-bearing. Every pointer target, the pane-origin
/// precondition and the scroll threshold are derived from that cell size, so all
/// of them are unmoved. And because `cell_h == ascent + descent`, the baseline
/// offset `render::draw_glyph` applies (`baseline - ascent`, which is
/// `(cell_h - ascent - descent) / 2`) is exactly `0.0`, so every glyph paints at
/// the same coordinates it did before and no OCR assertion's calibration changes.
///
/// `117` and `119` round to 44 as well; `118` is chosen because it is the middle
/// of that run (`37.3333 x 1.18 = 44.0533`, `0.447` clear of the 44.5 boundary and
/// `0.553` clear of 43.5), so it is the most robust of the three to a float
/// difference in a later Pango. `117` gives `43.68`, only `0.18` from a boundary —
/// do not use it.
///
/// A `0` here would also reproduce today's cell height at any size, since the
/// measured leading is zero and the non-positive branch is then
/// `ceil(ascent + descent)`. It is deliberately NOT used: a positive percentage
/// exercises the shipped code path these smokes exist to protect, and a `0` would
/// leave that path untouched by any smoke at all.
///
/// **Changing this moves every derived pixel target in this binary.** Re-measure
/// before touching it; do not adjust it to make an assertion pass.
const SMOKE_LINE_HEIGHT: &str = "118";

/// How many tiny marker `.md` files `window_renders_typed_text_under_cage` drops
/// into its temp cwd before launching the app, purely so the tree sidebar has rows
/// of text to draw.
///
/// This is an OCR-robustness fixture, not a product requirement, and it weakens
/// nothing: the assertion, `OCR_WORD`, [`SMOKE_FONT_PT`], the `SCENARIO` string
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
/// The `adw::HeaderBar` renders the app's WORKING DIRECTORY, so the title is part
/// of the page `tesseract` calibrates against — a third input to this fixture's
/// arithmetic, alongside the filler rows and the editor grid. It was measured to
/// matter, and the measurement is recorded here rather than left to be re-derived:
///
/// | arm | five consecutive capture md5s | size |
/// | --- | --- | --- |
/// | title rendering the run-varying temp path | all five DIFFERENT | 34.2–35.0 KB, varying |
/// | `set_title` suppressed (the base page) | `5ea4d070…` ×5, byte-identical | 26074 ×5 |
///
/// With a run-varying title — a directory named from `std::env::temp_dir()` + the
/// pid + a 19-digit nanosecond timestamp, so the header bar draws a different string
/// on every run — `window_renders_typed_text_under_cage`'s OCR is STOCHASTIC: the
/// isolated leading `h` of `OCR_WORD` misclassified (`ello` / `ﬁello`, the
/// documented signature above) in 2 of ~93 runs. With the title suppressed the
/// capture is bit-for-bit reproducible.
///
/// **The fix is not to raise this number.** [`make_temp_cwd`] and [`cage_command`]
/// hold the rendered title at the fixed string `~/project` for every smoke (a
/// fixed-name cwd under a `HOME` that is its own parent), so the captured page is
/// byte-identical across runs and the five-file floor measured above means what it
/// meant when it was measured. Letting a run-varying path back into the title
/// reintroduces that ~2% flake, and raising this count would NOT fix it: more filler
/// files can only make a stochastic page misread less often, never deterministically.
///
/// ## Why it belongs here at all
///
/// The assertion depends on the sidebar's page density — the five-file floor
/// measured above. Writing these files makes that dependency explicit and deliberate,
/// and mirrors what most other smokes get incidentally from the fixture files they
/// drop into their own temp cwds. (Five drop no fixture files at all: the empty-state
/// smoke hides the sidebar entirely, the `:qa` smoke asserts on an exit status rather
/// than OCR, and the click, drag and scroll pointer smokes build their buffer with
/// `:put` and assert on Neovim-written readback files rather than on pixels.)
///
/// Several smokes write these fillers DELIBERATELY rather than relying on their
/// own fixtures for density — the keyboard crash-recovery smoke and all three
/// state-persistence smokes, the last of which
/// (`window_title_shows_the_working_directory_under_cage`) asserts on a single
/// short header-bar title over an otherwise thin page.
const SIDEBAR_FILLER_COUNT: usize = 9;

/// Names of the [`SIDEBAR_FILLER_COUNT`] marker files. Only the FILENAME is ever
/// rendered (they are listed in the sidebar and never opened), so the names are
/// word-like and all-lowercase — the shapes `tesseract` reads best — and short
/// enough to fit the sidebar pane without being ellipsized.
///
/// The naming constraint is load-bearing. No name may contain `OCR_WORD`: a filler
/// row in the sidebar would then satisfy this smoke's assertion on its own, and the
/// test would pass green while the editor drew nothing — a silent false positive.
/// They are likewise kept clear of every other smoke's OCR constants — the asserted
/// SUBSTRINGS, not the fixture filenames: `pickertarget`, `pickone`/`picktwo`,
/// `treenode`, `paneleft`/`paneright`, `htopfile`/`hbotfile`,
/// `focusone`/`focustwo`/`zzleft`, `closekeep`/`closegone`,
/// `triaone`/`tribtwo`/`tricthree`, `folder`, `zzfresh`/`newbufbase`,
/// `quitkeep`/`quitgone`, `chordkeep`/`chordgone`, `wedgestay`/`wedgeclick`,
/// `restorealfa`/`restorebravo`/`restorecharlie`, `solozulu`/`soloalfa`,
/// `titlezulu`/`titlehome`, `bufpickalfa`/`bufpickbravo`, `responding`,
/// `stallfixture`, `projsearchalfa`/`projsearchdelta`/`zzmango`/`psrxray`, `ortango`,
/// `navbackalfa`/`navbackbravo`, `sbtangoleaf`/`sidebarkeepalfa`,
/// `cdtango`/`cdrootzulu`/`cdouterhome`, `zzbodyoscar`/`zzothertango`/`zzlivekilo`,
/// the one ORDINARY WORD in the whole table, `preview`,
/// `zzunsavedromeo`/`zzcoversierra`, `zzdrawermark`/`zzeditorfill`, and
/// `zztabalfa`/`zztabbravo` — so a stray filler can never satisfy one of those
/// either. `sidebar_filler_names_cannot_satisfy_any_smoke_assertion`
/// enforces this mechanically against `ASSERTED_OCR_TERMS`, which holds the consts
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

// Pointer smokes: every target is relative to a probed cell and asserted as a delta, so no origin
// constant matters; `assert_probe_cell_and_pane_origin` fails loudly on geometry drift.

/// x of the point every pointer smoke probes first, in absolute OUTPUT pixels
/// (`zwlr_virtual_pointer_v1.motion_absolute` takes output pixels; there is no CSD
/// or surface offset under cage — `motion 640 360` was measured arriving at the GTK
/// client as surface-local `640.00000000, 360.00000000`).
///
/// MEASURED 2026-08-04 on this host at `WOLF_FONT_PT` = [`SMOKE_FONT_PT`] on a
/// 1280x720 cage output: `(640, 360)` maps to Neovim cell `(line 5, col 17)`, against
/// a pane of 11 visible rows. That satisfies every geometry precondition at once —
/// `l0 >= 2 && c0 >= 2` (5 and 17); `c0 + CLICK_DX(7) = 24 <= 30`;
/// `c0 + DRAG_DX(9) = 26 <= 30`; and `l0 + DRAG_DY(5) = 10 <= 11`. It is also
/// comfortably inside the LEFT pane after a 50/50 vertical split, which the wedge
/// smoke depends on.
///
/// **The row budget is the tight one, and it is tighter than it was.** The per-pane
/// chrome costs three rows, so `assert_fixture_fits`'s
/// `probe_line + rows_below <= winheight` clears by a SINGLE row (10 against 11)
/// where it once cleared by one against 14. Anything that grows the statusbar strip
/// or the padding spends that row, and the failure lands on `assert_fixture_fits`
/// with a message naming the row budget — which is the right place, but it is one
/// row away rather than a comfortable margin. The columns are not close: 26 against a
/// 30-character fixture line.
///
/// If a phase ever lands within half a pixel of a cell boundary and skews a delta
/// assertion, the symptom is REPRODUCIBLE, not flaky (fixed output size,
/// `FONTCONFIG_FILE`-pinned DejaVu, pinned GTK/libadwaita): shift this by 2 px and
/// record why.
const PROBE_X: i32 = 640;

/// y of the probe point, in absolute output pixels. See [`PROBE_X`].
const PROBE_Y: i32 = 360;

/// x of the editor pane's TEXT GRID origin in output pixels, as DERIVED from the
/// probe click: `PROBE_X - (c0 - 1) * cell_w`.
///
/// MEASURED 2026-08-04 on a **1280x720** cage output at `WOLF_FONT_PT` =
/// [`SMOKE_FONT_PT`]: the probe at `(640, 360)` lands on Neovim cell
/// `(line 5, col 17)` with `cell_w = 22.4765625`, `cell_h = 44.0`, giving a derived
/// origin of `(280.375, 184.0)`.
///
/// ## Why it moved, and why the previous value was worse than wrong
///
/// The per-pane chrome — `editor_inset::EDITOR_PADDING_PX` and the statusbar strip —
/// puts the text grid inside the pane widget instead of at its corner. In x that is
/// the padding alone: the previous value, `257.8984375`, differs from this one by
/// exactly `22.4765625`, one whole `cell_w`, because the probe landed one column
/// earlier while the true origin moved 20 px right.
///
/// **The x band did not fail on that**, and that is the point worth recording. The
/// shift was `+22.48` px inside a `+/-34.48` px band (`PANE_ORIGIN_TOLERANCE_PX` plus
/// one `cell_w`), so the old value stayed green while describing a layout that no
/// longer existed — the same class as a test that cannot detect its own failure. It
/// is re-derived here deliberately, from a run, and NOT because anything went red.
/// The y band did fail, loudly, as designed.
///
/// The value carries the probe click's unknown SUB-CELL PHASE, so it OVER-estimates
/// the true origin by up to one cell — which is exactly why the band adds `cell_w` /
/// `cell_h` (see [`PANE_ORIGIN_TOLERANCE_PX`]). `-1.0` is the deliberate impossible
/// placeholder; re-deriving the value is a matter of putting `-1.0` back and reading
/// the band failure's message.
const PANE_ORIGIN_X_EXPECTED: f64 = 280.375;

/// y of the editor pane's TEXT GRID origin in output pixels, as DERIVED from the
/// probe click: `PROBE_Y - (l0 - 1) * cell_h`. See [`PANE_ORIGIN_X_EXPECTED`] for the
/// measurement, its date and the cage output size it was taken on.
///
/// MEASURED 2026-08-04: `184.0`, up from `52.0`. In y the chrome is the whole
/// statusbar strip plus the top padding, so this is the header bar's height plus the
/// strip plus `EDITOR_PADDING_PX`, plus the probe's sub-cell phase.
///
/// **This pair is the TEXT GRID's corner, and [`OVERLAY_ORIGIN_X_EXPECTED`] /
/// [`OVERLAY_ORIGIN_Y_EXPECTED`] are the pane AREA's.** They are two different
/// points, measured two different ways — this pair from a click-to-cell readback,
/// that pair from the overlay's own allocation — and the per-pane chrome is the gap
/// between them. They look interchangeable and are not: a derivation that wants the
/// area's corner (the crash overlay's allocation, a divider seed) and reads this pair
/// instead is out by a statusbar strip and a padding, which is enough to put a click
/// outside a 320x64 button. Neither pair may be derived from the other; each is
/// measured, and each carries its own provenance.
///
/// The one place the distinction is currently invisible is the divider smokes' seeds,
/// which read this pair as the area's left edge. They are documented as SEEDS ONLY
/// and the bisection converges from either, so nothing there is wrong today — but the
/// premise is the same one, and if a seed ever stops straddling the divider this is
/// the first place to look.
const PANE_ORIGIN_Y_EXPECTED: f64 = 184.0;

/// Half-width of the band the derived pane origin must fall inside.
///
/// ACCEPTANCE CRITERION: it must be narrow enough that a `+20,+20` surface offset
/// FAILS, so it has to be `< 20.0`. `12.0` also comfortably contains the measured
/// 15.2 px (x) / 16 px (y) spread of derived origins across three probe points —
/// that spread is the SUB-CELL PHASE of the probe click, which is why the band adds
/// `cell_w` / `cell_h` on top. Those two terms are an unavoidable phase allowance,
/// NOT slack, and must not be grown to make a failing run pass.
const PANE_ORIGIN_TOLERANCE_PX: f64 = 12.0;

/// x of the crash OVERLAY's top-left corner in output pixels — the pane area's own
/// left edge, which is the sidebar's right edge plus the `gtk::Paned` handle.
///
/// A SEPARATE measurement from [`PANE_ORIGIN_X_EXPECTED`], and it must stay separate.
/// That constant is derived from a click-to-cell readback, so it names the TEXT
/// GRID's corner; this one names the WIDGET's. The per-pane chrome is the difference,
/// and the two are only ever equal in a build that has none.
///
/// Measured 2026-08-04 on a **1280x720** cage output, from a temporary tick callback
/// that printed every distinct overlay allocation across the crash, the restart and
/// the click, and reproduced on a second run: origin `(251, 47)`, allocation
/// `1029 x 673`. The two agree by construction — `1280 - 251 = 1029` and
/// `720 - 47 = 673` — so they are one measurement, not two.
///
/// ## The cross-check, which is what makes the number trustworthy
///
/// It is confirmed a second, independent way: through
/// `wolf_gnome::restart_button_rect`, against the rendered button as inspected on a
/// retained capture.
///
/// `restart_button_rect(1029, 673)` is `(354.5, 344.5, 320, 64)` —
/// `x = (1029 - 320) / 2`, and `y = (673 - 144) / 2 + 80` where 144 is the fixed
/// message + gap + button block and 80 is the message plus the gap. Offset by the
/// origin that gives absolute `x 605.5..925.5`, `y 391.5..455.5`, against an
/// independently inspected render of `x 605..925`, `y 392..456`. The two routes agree
/// to within the half pixel GTK snaps a centred block of odd leftover height by, which
/// is as close as a pixel-inspected render and a real-valued rect can come.
const OVERLAY_ORIGIN_X_EXPECTED: f64 = 251.0;

/// y of the crash OVERLAY's top-left corner in output pixels — the `adw::HeaderBar`'s
/// height, and nothing else. See [`OVERLAY_ORIGIN_X_EXPECTED`] for the measurement,
/// its date, the cage output size, and the button cross-check that confirms it.
///
/// This is the ONE-TAB value, and it is the value every other smoke here uses,
/// because `adw::TabBar::set_autohide` is left at `true`: at one page the bar
/// allocates zero height and the window's geometry is byte-identical to a build with
/// no tab layer at all. See [`TAB_BAR_HEIGHT_EXPECTED`] for the two-page figure and
/// for why an `is_visible()` assertion on the bar would say nothing.
const OVERLAY_ORIGIN_Y_EXPECTED: f64 = 47.0;

/// Allocated height, in output pixels, of the `adw::TabBar` once a SECOND page
/// exists — the strip between the `adw::HeaderBar` and the `gtk::Paned`.
///
/// MEASURED under `cage` on a 1280x720 output at [`SMOKE_FONT_PT`], from a temporary
/// tick callback printing the bar's and the window overlay's allocations, exactly the
/// procedure [`OVERLAY_ORIGIN_X_EXPECTED`] records. Three facts came out of it, and
/// all three are load-bearing for
/// `native_tabs_open_switch_and_close_under_cage`:
///
/// * **The bar is 47 px tall at two pages**, and the pane area's top edge moves from
///   [`OVERLAY_ORIGIN_Y_EXPECTED`] (47) to 94 — the two are equal by coincidence of
///   theme metrics, not by construction, so neither may be derived from the other.
/// * **The bar spans the FULL 1280 px output, not the pane area.** It is appended to
///   the window's vertical `gtk::Box` ABOVE the `gtk::Paned`, so it starts at x = 0
///   and is not inset by the sidebar. A click derived from
///   [`OVERLAY_ORIGIN_X_EXPECTED`] would land ~251 px right of where it was meant to.
///   With two equal tabs the measured centres are x = 320 and x = 960; see
///   `TABS_FIRST_TAB_CENTRE_FRACTION`.
/// * **The bar ANIMATES in over roughly one second** — successive frames read 0, 2,
///   13, … , 47. Anything that depends on its final height, whether an injected click
///   or a `winheight(0)` readback, must be taken AFTER it settles; the scenario's
///   `TABS_BAR_SETTLE` is that budget.
///
/// A fourth measurement is recorded here as a NEGATIVE, because it looks like an
/// assertion and is not one: `bar.is_visible()` reads `true` even at ONE page.
/// Autohide works by allocating zero height rather than by hiding the widget, so an
/// `is_visible()` assertion would hold in both builds and discriminate nothing. The
/// autohide guarantee is instead enforced by the OTHER pointer smokes, whose
/// derived targets all rest on [`OVERLAY_ORIGIN_Y_EXPECTED`] and would break loudly
/// through [`assert_probe_cell_and_pane_origin`] if a one-tab window's pane origin
/// ever moved.
///
/// Nothing re-derives this number on a green run. Its only live check is the
/// consequence of the click composed from it: a wrong height puts the injected point
/// off the bar, the tab is never selected, and the scenario exits 25.
const TAB_BAR_HEIGHT_EXPECTED: f64 = 47.0;

/// The smallest Neovim `line`/`col` the probe click may report. `(1, 1)` is where
/// the cursor already is, so anything below this means the press never landed and
/// every delta downstream would be measuring a no-op.
const MIN_PROBE_CELL: i64 = 2;

/// Width, in characters, of every line the `:put` fixture writes. See the
/// `fill_buffer` comment in [`POINTER_PREAMBLE`] for the three empirical
/// constraints that pin this number.
const FIXTURE_LINE_WIDTH: i64 = 30;

/// The `<linux/input-event-codes.h>` `BTN_LEFT` code, as the helper's `press` /
/// `release` grammar takes it. Passed into every pointer scenario as `$BTN_LEFT` so
/// this const is the single source of truth rather than a `272` retyped four times.
const POINTER_BUTTON_LEFT: &str = "272";

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
/// required unconditionally, not just by the pointer-driven smokes: a devShell missing
/// it is an unprovisioned devShell, and the project policy is to fail loud rather
/// than let the gate pass green while proving less than it claims.
const REQUIRED_TOOLS: [&str; 6] = [
    "cage",
    "dbus-run-session",
    "wtype",
    "grim",
    "tesseract",
    "wolf-pointer-inject",
];

/// Confirms `tool` resolves on `PATH` (via `command -v`). Returns true if found.
fn tool_on_path(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Takes one of this process's [`CAGE_SLOTS`] (blocking until one is free), then
/// panics with an actionable message unless every smoke precondition holds: the six
/// [`REQUIRED_TOOLS`] are on `PATH`, and `WOLF_NVIM_BIN` and `WOLF_RG_BIN`
/// are set. Returns the smoke's [`CageRuntime`]: its slot plus a fresh private runtime
/// directory, which every capture and readback path is joined onto and which
/// [`cage_command`] hands the cage as `XDG_RUNTIME_DIR`.
///
/// Bind the result to a named variable for the whole body; `let _ =` drops it at
/// once, freeing the slot and deleting the directory.
///
/// Both binary variables are checked here, for every smoke, rather than only in the
/// smokes that need them. `cage_command` inherits this process's environment, so a
/// variable missing here is a variable the app under test will not have either, and
/// the failure it produces there is silent: `WolfCore::start_search` does not
/// panic and does not block on an unset `WOLF_RG_BIN` — it hands back a session that
/// is already complete with zero matches. Checking unconditionally is what turns
/// "the result list stayed empty" into "the devShell was not entered".
fn require_smoke_env() -> CageRuntime<'static> {
    let slot = CAGE_SLOTS.acquire(smoke_jobs());

    let missing: Vec<&str> = REQUIRED_TOOLS
        .iter()
        .copied()
        .filter(|t| !tool_on_path(t))
        .collect();
    assert!(
        missing.is_empty(),
        "wolf-gnome headless smoke test: missing required tool(s) {missing:?} on PATH. \
         These are provided by the flake devShell smoke toolchain (cage, dbus-run-session, \
         wtype, grim, tesseract, wolf-pointer-inject). Run via `nix develop --command cargo test -p \
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

    match std::env::var("WOLF_RG_BIN") {
        Ok(v) if !v.trim().is_empty() => {}
        _ => panic!(
            "wolf-gnome headless smoke test: WOLF_RG_BIN is not set (or empty). It must point \
             at the pinned ripgrep binary exported by the flake devShell; rg is NOT taken from \
             PATH. This is a LOUD failure rather than a skip because the alternative is silent: \
             `WolfCore::start_search` resolves that variable alone and, when it is unset, \
             returns an ALREADY-COMPLETE session carrying zero matches and the error \
             \"WOLF_RG_BIN unset\". `search_panel_streams_results_and_opens_the_match_under_cage` \
             would then find an empty panel and fail on its first sidebar assertion, pointing at \
             the panel when the real cause is an unprovisioned shell. Run via `nix develop \
             --command cargo test -p wolf-gnome`."
        ),
    }

    CageRuntime::fresh(slot)
}

/// Everything the pointer scenarios have in common, as one `sh` fragment:
/// the teardown trap, the command FIFO, the `wolf-pointer-inject` helper, the
/// poll-for-ack protocol, the readback-file poll, the buffer fixture, the app
/// launch + readiness wait, and one initial in-pane `motion`.
///
/// This is a DELIBERATE deviation from this binary's "one self-contained scenario
/// const per smoke" style. The preamble is ~90 lines of FIFO / ack / poll plumbing,
/// and a copy per smoke would be a place per smoke to fix a poll bug — the exact
/// shape that lets a harness rot in all but one of its instances unnoticed. Each smoke's own
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
/// ## Exit codes (following the `COLON_QA_SCENARIO` precedent)
///
/// * `11` — the app died before it was ready for input.
/// * `12` — the helper never became ready.
/// * `13` — a helper command was not acked within budget.
/// * `14` — a Neovim readback file never appeared.
/// * `15` — the helper died mid-scenario.
/// * `40` — the app never became ready for input ([`READINESS_PRELUDE`]).
///
/// Each of `12`–`15` echoes a one-line reason AND the helper's stderr, because when
/// the helper is the failing party its own message ("no
/// `zwlr_virtual_pointer_manager_v1` in the registry", "more than one `wl_output`") is
/// the whole diagnosis. `11` and `40` come from the shared `wait_ready`, which echoes
/// its own reason only: neither is about the helper.
const POINTER_PREAMBLE: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
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
# MEASURED against the Linux flake's nvim 0.12.6, three back-to-back calls (press at
# one cell, drag and release at another):
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
wait_ready "$app_pid" 11 "wolf-gnome pointer smoke"

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

/// Creates a fresh, unique temp directory to serve as the launched app's working
/// directory. The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so
/// this is the directory the file-search traversal walks and the directory the
/// captive nvim runs in (the recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd).
///
/// The cage smokes derive their working directory from this — the window
/// title smoke and the typed-`:cd` smoke use the result as the base for the NESTED
/// directories their `~` abbreviation needs, the per-pane statusbar smoke hangs its
/// fixture directory off the parent, and the relaunch smokes share one of these
/// directories across BOTH of their launches, which is the whole scenario. A fresh
/// cwd per run is MANDATORY, not tidiness:
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
/// harmless, because no future run resolves to that hash — and that state dir is
/// this cwd's own sibling ([`smoke_state_dir`]) rather than the developer's real
/// `~/.local/state/wolf/`.
///
/// ## Why the returned directory has a FIXED name under a unique parent
///
/// The returned path is `<unique>/project`, not `<unique>`: the uniqueness lives
/// entirely in the PARENT, and the component the app ever renders is the constant
/// [`TEMP_CWD_BASENAME`]. That split exists for one reason, and it is an OCR
/// determinism reason rather than a filesystem one.
///
/// The `adw::HeaderBar` renders the app's working directory, so the temp
/// directory's NAME is page text in every capture. While
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
/// `window_renders_typed_text_under_cage` failed 2 of ~93 runs with the documented
/// signature, the isolated leading `h` of `OCR_WORD` misclassifying as `ello` /
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
    let seq = TEMP_CWD_SEQ.fetch_add(1, Ordering::Relaxed);
    // The unique part is the parent, drawn as `~` because it is the app's `HOME`. Created
    // non-recursively, so two smokes computing the same name fail here instead of sharing a cwd.
    let parent = std::env::temp_dir().join(format!("{TEMP_CWD_PREFIX}{pid}-{nanos}-{seq}"));
    let dir = parent.join(TEMP_CWD_BASENAME);
    for level in [&parent, &dir] {
        std::fs::create_dir(level)
            .unwrap_or_else(|e| panic!("failed to create temp cwd {}: {e}", level.display()));
    }
    dir
}

/// Distinguishes the [`make_temp_cwd`] directories of one test process, which run
/// concurrently and can read the same nanosecond clock value.
static TEMP_CWD_SEQ: AtomicUsize = AtomicUsize::new(0);

/// The fixed leading part of every [`make_temp_cwd`] directory's UNIQUE PARENT; the
/// rest is the process id, a nanosecond timestamp and a sequence number, i.e. ASCII
/// digits and `-` only.
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
/// `temp_cwd_name_cannot_satisfy_any_smoke_assertion` keeps it clear of every
/// `ASSERTED_OCR_TERMS` entry, and reads it from here so a rename cannot drift past
/// the guard.
const TEMP_CWD_PREFIX: &str = "wolf-gnome-smoke-";

/// The FIXED basename of every [`make_temp_cwd`] directory — the app's actual working
/// directory, and the only part of it the header-bar title ever renders (as
/// `~/project`, because [`cage_command`] makes the unique parent the app's `HOME`).
///
/// Being fixed is the whole point: it is what makes the captured page byte-identical
/// across runs, which every OCR assertion in this binary depends on. See
/// [`make_temp_cwd`] for the measurement that forced it and
/// [`SIDEBAR_FILLER_COUNT`] for why page stability matters at all.
///
/// Because it IS drawn, it carries the [`SIDEBAR_FILLER_FILES`] hazard for real:
/// were it named `folder`, `empty_state_shows_open_folder_button_under_cage` would
/// be satisfied by the header bar with no status page drawn at all.
/// `temp_cwd_name_cannot_satisfy_any_smoke_assertion` checks it against every
/// `ASSERTED_OCR_TERMS` entry, in both directions.
///
/// It must also stay SHORT and word-like: it is added glyph mass on every page, and
/// a long or noisy title would move the OCR calibration this binary's fixtures are
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
/// the durable per-project swap dir (`swap/<hash of cwd>/`), the three MRU sqlite
/// files, and `state.json` itself.
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
    smoke_state_dir_at(cwd, "")
}

/// [`smoke_state_dir`] with an extra name suffix, so ONE scenario can hand its two
/// launches two DIFFERENT state directories — exactly the split
/// [`smoke_config_dir_at`] already is, and derived here for the same reason: the
/// sibling-not-child rule lives in one place.
///
/// ## When a shared state dir is right, and when it is a coupling
///
/// [`cage_command`] hands both launches of a two-launch scenario the one directory
/// this returns for `""`, and for the RELAUNCH smokes that sharing IS the
/// scenario: launch 2 must read exactly the `state.json` launch 1 wrote.
///
/// For a two-launch smoke whose subject is not restore, the same sharing is a
/// coupling that silently changes what launch 2 is measuring.
/// `preview_document_follows_the_configured_appearance_under_cage` is the case
/// that forced this split: its launch 1 quits with a preview pane open, so its
/// `state.json` records a two-leaf layout with a live preview leaf, and launch 2
/// would then restore TWO panes before its own Preview press. That press's
/// split would make THREE panes rather than two, and every derived rectangle in that
/// smoke — all of which assume the preview is the RIGHT half of two — would be aimed
/// at the wrong pane. Worse, it would make a colour claim depend on
/// the restore path, which is the restore smokes' subject, not this one's.
///
/// A suffix per launch is what keeps each launch's own persistence its own.
fn smoke_state_dir_at(cwd: &Path, suffix: &str) -> PathBuf {
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
    let dir = parent.join(format!("{}-state{suffix}", name.to_string_lossy()));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the smoke state dir {}: {e}. Without it the app under cage falls \
             back to the developer's real state dir and would read and overwrite their real \
             state.json.",
            dir.display()
        )
    });
    seed_smoke_spell_override(&dir);
    dir
}

/// The `init_user.lua` every smoke's Neovim is given, which turns spell checking
/// back OFF. Written into the per-run state dir, never into a real one.
///
/// Clearing the augroup is what disables it. The option is set from a `FileType`
/// autocmd on `WolfSpell`, so assigning `spell = false` here alone would be
/// overwritten on the next buffer that autocmd sees. This file is sourced LAST, after
/// every Wolf default, so the group exists by the time it runs. The window sweep
/// covers any buffer that reached its filetype before this point.
const SMOKE_INIT_USER_LUA: &str = "\
-- Seeded by the wolf-gnome smoke harness; see `seed_smoke_spell_override`.
vim.api.nvim_create_augroup(\"WolfSpell\", { clear = true })
for _, win in ipairs(vim.api.nvim_list_wins()) do
\tvim.wo[win].spell = false
end
";

/// Pins every smoke's Neovim to spell-OFF, through the app's own user-override hook.
///
/// ## Why the smokes cannot run with spell on
///
/// The captive `init.lua` turns window-local `'spell'` ON for markdown, and EVERY
/// fixture heading these smokes OCR is a coined word — `SOLOZULU`, `STALLFIXTURE`,
/// `PROJECTNEEDLE` — so nvim draws a red undercurl along the glyph baseline. The
/// page is drawn correctly; tesseract is what cannot read it. Measured: six OCR
/// smokes failed with their heading plainly legible in the retained PNG, each
/// reporting "the GTK pane did not paint it" about a pane that had.
///
/// Pinned rather than inherited, for the reason `GTK_IM_MODULE` is pinned in
/// [`cage_command`]: every OCR threshold in this binary was calibrated against an
/// undercurl-free page, so leaving spell on would leave the heading assertions
/// nearer their margin than the number each was tuned to, and the ones that still
/// passed would be passing by luck.
///
/// ## Why this is a seed and not an app change
///
/// Spell on prose IS the shipped behaviour and has its own coverage in
/// `wolf-core`'s `spell.rs` and `spell_durable.rs`. What belongs here is the smokes'
/// observation channel, not the editor's defaults, so the override goes in through
/// `runtime_user/init_user.lua` exactly as a user's would. A pre-existing
/// `init_user.lua` is sourced and never rewritten by the app, so seeding it before
/// launch also suppresses the template the app would otherwise write there.
///
/// Called from [`smoke_state_dir_at`], the one place smoke state dirs are derived,
/// and from the typed-`:cd` smoke, the one smoke that builds its own.
fn seed_smoke_spell_override(state_dir: &Path) {
    let dir = state_dir.join("runtime_user");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the smoke runtime_user dir {}: {e}. It carries the seeded \
             init_user.lua that turns spell checking off; without it every OCR assertion on a \
             coined fixture heading reads an undercurl. See `seed_smoke_spell_override`.",
            dir.display()
        )
    });
    let file = dir.join("init_user.lua");
    std::fs::write(&file, SMOKE_INIT_USER_LUA).unwrap_or_else(|e| {
        panic!(
            "failed to seed the smoke init_user.lua at {}: {e}. See \
             `seed_smoke_spell_override`.",
            file.display()
        )
    });
}

/// The config directory one smoke's app is given via `WOLF_CONFIG_DIR`: a
/// **sibling** of `cwd`, named after it with a `-config` suffix, created eagerly
/// and SEEDED with a `wolf-config.yaml`.
///
/// ## Why this exists at all — it is isolation, not hygiene
///
/// `wolf_core::preferences_dir()` resolves `WOLF_CONFIG_DIR` first and otherwise
/// falls back to `dirs::config_dir()`, which honours **`XDG_CONFIG_HOME`** ahead of
/// `$HOME/.config`. [`cage_command`] points both at the smoke home's `.config`, so
/// the developer's real `wolf-config.yaml` is out of reach either way; a smoke that
/// spawned `cage` without that helper would inherit the developer's `XDG_CONFIG_HOME`
/// and read — and, on a first run, WRITE — their file.
///
/// Worse, and true on every host: `load_or_create` CREATES the file when it is
/// absent. An unisolated smoke that found no config would be handed one carrying
/// `default_config()` — family `iA Writer Mono S`, size 10 — and that family
/// resolves under the pinned `FONTCONFIG_FILE`, which exposes it. Every glyph in
/// every OCR smoke would change shape, and the change would arrive on whichever
/// machine happened not to have the file yet.
///
/// ## Why the file is SEEDED rather than left absent
///
/// A seeded file makes `load_or_create` READ rather than write, so the smoke is
/// deterministic and leaves nothing behind that a second run would read
/// differently. It pins the family to [`SMOKE_FONT_FAMILY`] — the one
/// [`cell_metrics_for_smoke`] measures against — the line-height to
/// [`SMOKE_LINE_HEIGHT`], whose doc comment carries the measurement behind the
/// number, and the appearance to [`SMOKE_APPEARANCE`], whose reason lives in
/// [`write_smoke_config`]. The `font-size` it carries is inert while
/// `WOLF_FONT_PT` wins (`config_seam::resolve_font_pt`'s documented precedence),
/// and is written anyway so the file is honest about what it configures.
///
/// ## Why a SIBLING and not a child
///
/// The same reason [`smoke_state_dir`] is: a child of `cwd` would be walked by the
/// app's own `FileSearch`, so `wolf-config.yaml` would appear as a tree-sidebar row
/// and a picker candidate, changing the page density every OCR assertion is
/// calibrated against.
fn smoke_config_dir(cwd: &Path) -> PathBuf {
    smoke_config_dir_at(cwd, "", SMOKE_FONT_PT, SMOKE_APPEARANCE)
}

/// [`smoke_config_dir`] with an extra name suffix and an explicit `font-size` and
/// `appearance`.
///
/// Some smokes need TWO seeded config directories in one scenario, such as
/// `config_font_size_changes_the_grid_under_cage` at two different font sizes
/// and `configured_appearance_reaches_neovims_background_under_cage` at two
/// different appearances. Deriving them all here keeps the sibling-not-child rule
/// and the seeded file's shape in one place; see [`smoke_config_dir`] for why each
/// matters.
fn smoke_config_dir_at(cwd: &Path, suffix: &str, font_pt: &str, appearance: &str) -> PathBuf {
    let name = cwd.file_name().unwrap_or_else(|| {
        panic!(
            "smoke cwd {} has no final path component, so no `<name>-config` sibling can be \
             derived for it. Every smoke's cwd comes from `make_temp_cwd`, which always produces \
             one.",
            cwd.display()
        )
    });
    let parent = cwd.parent().unwrap_or_else(|| {
        panic!(
            "smoke cwd {} has no parent directory, so no SIBLING config dir can be derived for \
             it. A child would be walked by the app's own `FileSearch` and is not an acceptable \
             fallback; see `smoke_config_dir`.",
            cwd.display()
        )
    });
    let dir = parent.join(format!("{}-config{suffix}", name.to_string_lossy()));
    write_smoke_config(&dir, font_pt, appearance);
    dir
}

/// Creates `dir` and writes a `wolf-config.yaml` into it pinning
/// [`SMOKE_FONT_FAMILY`], `font_pt`, [`SMOKE_LINE_HEIGHT`] and `appearance`.
///
/// Split out from [`smoke_config_dir`] because some smokes need the same file at
/// two DIFFERENT values — such as `config_font_size_changes_the_grid_under_cage`
/// at two font sizes and `configured_appearance_reaches_neovims_background_under_cage`
/// at two appearances — and the seeded shape must not be written twice by hand.
/// The three font fields are required by `wolf_core`'s `ConfigSection`: a partial
/// section fails to deserialize and `load_or_create` then falls back to the
/// built-in defaults, which is exactly the `iA Writer Mono S` outcome this whole
/// helper exists to prevent.
///
/// ## Why `appearance` is pinned, and why to [`SMOKE_APPEARANCE`]
///
/// `appearance` is the one seeded field that is NOT required — `wolf_core`'s
/// `ConfigSection` gives it a `#[serde(default)]` of `auto` — so an unpinned file
/// leaves every smoke running in `auto`. That is the same class of hazard the
/// family and the size are pinned against, one step further out: in `auto` the
/// resolved appearance comes from libadwaita's reading of the desktop, and under
/// `cage` there is no settings portal and no xsettings bridge to read. The
/// rendered page would then depend on the compositor's ABSENT theme state, which
/// is not a value this suite controls and not one it could re-derive after a
/// failure.
///
/// `dark` specifically, because it is what Neovim already defaults to under
/// `--embed`: pinning it pushes the grid to the background it was already at, so
/// NO OCR page moves. Pinning `light` would invert every one of the OCR smokes at
/// once, and their sparse-text segmentation is calibrated on a light-on-dark grid.
/// Changing this is therefore a change to every OCR assertion in this binary; do not
/// adjust it to make one of them pass.
fn write_smoke_config(dir: &Path, font_pt: &str, appearance: &str) {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the smoke config dir {}: {e}. Without it the app under cage resolves \
             its preferences directory from XDG_CONFIG_HOME or $HOME and would read — or create — \
             the developer's real wolf-config.yaml.",
            dir.display()
        )
    });
    let path = dir.join(CONFIG_FILE_NAME);
    let yaml = format!(
        "config:\n  font-family: {SMOKE_FONT_FAMILY}\n  font-size: {font_pt}\n  \
         line-height: {SMOKE_LINE_HEIGHT}\n  appearance: {appearance}\n"
    );
    std::fs::write(&path, yaml).unwrap_or_else(|e| {
        panic!(
            "failed to write the smoke config {}: {e}. An absent file is NOT a benign fallback: \
             `load_or_create` would create one carrying `default_config()` — family \
             `iA Writer Mono S`, size 10 — and that family resolves under the pinned \
             FONTCONFIG_FILE, so every glyph in every OCR smoke would change.",
            path.display()
        )
    });
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
/// post-mortem. Losing it costs one cold launch, which is time spent in `wait_ready`
/// and nothing else.
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
/// smoke of an invocation pays a cold cache and the rest run warm. It does NOT buy:
/// warmth across invocations — the first smoke of EVERY invocation is cold, which is
/// acceptable because a cold cache costs launch time, not correctness (below).
///
/// ## Why this exists (measured, not inferred)
///
/// Before [`cage_command`] set `HOME`, the app under `cage` inherited the developer's
/// real `$HOME`, and therefore their real, long-warm `~/.cache`. Redirecting `HOME` to
/// the per-run [`smoke_home`] — which is load-bearing for OCR determinism and must not
/// be undone — silently made `$HOME/.cache` a FRESH directory on every smoke, so mesa
/// rebuilt its shader cache (`mesa_shader_cache` + `radv_builtin_shaders`, ~4.9 MB)
/// from scratch on every single launch. That is pure added latency between spawning
/// the app and the window being able to accept keys. Measured as the shortest fixed
/// pre-input sleep that let the first keystroke land (20 iterations per cell), warm
/// launches needed between 0.20 and 0.25 s and cold ones between 0.30 and 0.35 s.
///
/// ## Why one cold launch per invocation is ACCEPTABLE, not a defect to fix
///
/// Because no scenario types on a fixed budget: `wait_ready` waits for the app's own
/// readiness file, so a cold cache makes that wait about a tenth of a second longer
/// and changes nothing else. Cross-invocation warmth is therefore a BONUS, not a
/// requirement, and chasing a `TMPDIR`-proof location would be a design change bought
/// with no measured benefit.
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
/// match its neighbours: every launch would then rebuild the shaders, a cost paid on
/// every smoke for no isolation it does not already have.
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
/// The worst case of a name collision is a cold cache, which costs launch time inside
/// `wait_ready` and nothing else. A per-user or per-run name would buy nothing, while
/// giving up the one property this directory exists for.
fn smoke_shader_cache_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(SHARED_SHADER_CACHE_DIR);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the shared smoke shader cache dir {}: {e}. It is handed to every \
             smoke as XDG_CACHE_HOME so mesa's shader cache stays warm across runs; without it \
             every launch rebuilds ~4.9 MB of shaders and waits that much longer to become ready \
             for input. See `smoke_shader_cache_dir`.",
            dir.display()
        )
    });
    dir
}

/// The input-method module every smoke's app is launched with, pinned rather than
/// inherited.
///
/// ## Why a pin, and why it is not tidiness
///
/// `gtk::IMMulticontext` selects its slave from `GtkSettings:gtk-im-module`, which
/// GTK initializes from the `GTK_IM_MODULE` environment variable, and
/// [`cage_command`] inherits the developer's environment wholesale. On the
/// development host that variable reads `ibus`, the GTK 4 ibus module is not
/// installed, and GTK falls back to its built-in simple context — which is exactly
/// the context every input-method measurement was taken against.
/// That fallback is a property of THAT HOST, not of this harness.
///
/// The variable decides which context every smoke's typing goes through — and that
/// is ALL typing, not only accents, since the context filters plain printables with
/// no sequence pending. On a host
/// that does have the GTK 4 ibus module — a different NixOS configuration, or a
/// future flake that pulls ibus in for some other reason — the entire suite's typing
/// would silently reroute through an engine that cannot reach its daemon, because
/// the only bus a cage has is its own private one from `dbus-run-session`, where no
/// engine runs. **The attached context is
/// what makes the smokes' typing host-dependent at all, and this pin is what takes
/// that back.**
///
/// ## Why this value rather than removing the variable
///
/// Removing it lets GTK apply its own default selection, which on Wayland consults
/// the compositor for `zwp_text_input_manager_v3` — a `cage`/wlroots-dependent
/// branch that would make the choice depend on the compositor build rather than on
/// anything this repository pins. `gtk-im-context-simple` names the same context on
/// every host and every compositor.
///
/// ## What the pin does NOT buy
///
/// It makes no smoke evidence about any OUT-OF-PROCESS input method. None can be:
/// the flake ships no input-method engine and no GTK 4 ibus module, so nothing on a
/// cage's private bus could serve one. The pin
/// documents that constraint instead of leaving it to the ambient environment, and
/// `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage`'s
/// `GDK_DEBUG` note is the same species of per-smoke isolation.
const SMOKE_GTK_IM_MODULE: &str = "gtk-im-context-simple";

/// The shell every scenario starts with: [`cage_command`] prepends it, so no
/// scenario can forget it and none defines its own copy.
///
/// `wait_ready <pid> <died-code> <label>` first PRIMES the seat with a key-free
/// `wtype ""`, retried until it succeeds (cage may not be up yet), then polls until
/// `$WOLF_READY_FILE` names `<pid>`. The app writes it once its launch windows
/// accept keystrokes (see `notes/wolf-gnome-decisions.md`, "The readiness file").
///
/// The priming is load-bearing, not a liveness probe. A headless `cage` seat has no
/// keyboard until a virtual keyboard exists, and the first key the first virtual
/// keyboard sends after the window maps is lost (measured: the leading `i` of
/// `ihello` dropped in 5 of 10 runs without it). `wtype ""` creates a keyboard and
/// sends no key; the seat keeps the capability after it exits, and the app's
/// `no-keyboard` condition waits for the client to see it.
///
/// It exits `<died-code>` if the app dies first, and `READY_TIMEOUT_EXIT` (40) after
/// `READY_TRIES × READY_INTERVAL` (30 s) — killing the app first, because `cage`
/// outlives its shell while a client is still connected, and a scenario with no
/// teardown trap would otherwise hang the test. There is deliberately no settle
/// after it: a keystroke lost past readiness means the predicate lacks a condition,
/// and a sleep would hide which. Comparing the pid rather than testing for the file
/// is what lets a relaunch scenario's two launches share one path, and its
/// variables are all `_ready_`-prefixed so it cannot clobber a scenario's `tries`.
///
/// `wait_file_contains <file> <needle> <label>` polls until `<file>` holds `<needle>`
/// and returns 1 after 20 s, leaving the decision to the caller.
const READINESS_PRELUDE: &str = r#"
# A non-interactive bash as sh drops an inherited PS1, so a terminal smoke's prompt
# arrives as SMOKE_PS1 and is exported here for the shells the app starts.
if [ -n "${SMOKE_PS1:-}" ]; then PS1=$SMOKE_PS1; export PS1; fi

READY_TRIES=600
READY_INTERVAL=0.05
READY_TIMEOUT_EXIT=40

wait_ready() {
    _ready_n=0
    _ready_primed=
    while [ "$_ready_n" -lt "$READY_TRIES" ]; do
        if ! kill -0 "$1" 2>/dev/null; then
            echo "$3 (pid $1) exited before it was ready for input" >&2
            exit "$2"
        fi
        if [ -z "$_ready_primed" ] && wtype "" >/dev/null 2>&1; then
            _ready_primed=1
        fi
        _ready_got=
        read -r _ready_got 2>/dev/null <"$WOLF_READY_FILE" || true
        if [ "$_ready_got" = "$1" ]; then
            return 0
        fi
        _ready_n=$((_ready_n + 1))
        sleep "$READY_INTERVAL"
    done
    if [ -z "$_ready_primed" ]; then
        echo "$3: wtype could not prime the seat's keyboard within $READY_TRIES x ${READY_INTERVAL}s" >&2
        echo "  (no wtype on PATH, or cage never came up)" >&2
    else
        echo "$3: $WOLF_READY_FILE did not name pid $1 within $READY_TRIES x ${READY_INTERVAL}s;" >&2
        echo "  the app's 'wolf-ready:' stderr lines name the last unmet condition" >&2
    fi
    kill "$1" 2>/dev/null || true
    exit "$READY_TIMEOUT_EXIT"
}

FILE_CONTAINS_TRIES=200
FILE_CONTAINS_INTERVAL=0.1

wait_file_contains() {
    _wfc_n=0
    while [ "$_wfc_n" -lt "$FILE_CONTAINS_TRIES" ]; do
        if [ -s "$1" ] && grep -qF -- "$2" "$1"; then
            return 0
        fi
        _wfc_n=$((_wfc_n + 1))
        sleep "$FILE_CONTAINS_INTERVAL"
    done
    echo "$3: timed out waiting for $1 to contain $2" >&2
    return 1
}
"#;

/// Distinguishes the runtime directories of one test process's cages.
static SMOKE_RUNTIME_SEQ: AtomicUsize = AtomicUsize::new(0);

/// The fixed leading part of every [`CageRuntime`] directory's name; the rest is the
/// process id and [`SMOKE_RUNTIME_SEQ`]. It deliberately does not start with
/// [`TEMP_CWD_PREFIX`].
const RUNTIME_DIR_PREFIX: &str = "wolf-smoke-rt-";

/// The longest [`CageRuntime`] directory path, in bytes.
///
/// Every socket a cage's tree makes under `XDG_RUNTIME_DIR` must fit a 108-byte
/// `sun_path` (107 bytes plus the NUL). The longest measured under live cages,
/// sampled every second across full runs at 1 to 16 jobs, is WebKit's
/// `webkitgtk/a11y-proxy-XXXXXX`, 27 bytes past the directory; this leaves that plus
/// its separator and an 8-byte margin. Under `nix develop` the directory is 45–46
/// bytes.
const RUNTIME_DIR_MAX_BYTES: usize = 71;

/// The app's `WOLF_READY_FILE`, inside its cage's runtime directory.
const READY_FILE_NAME: &str = "wolf-ready";

/// Where [`cage_command`] sends the cage's stdout and stderr, inside its runtime
/// directory: the scenario's own diagnostics, the app's `wolf-ready:` lines and the
/// compositor's log.
const CAGE_LOG_NAME: &str = "cage.log";

/// How much of [`CAGE_LOG_NAME`] a failing smoke prints.
const CAGE_LOG_TAIL_LINES: usize = 80;

/// One smoke's cage slot plus the private `XDG_RUNTIME_DIR` its cage runs in.
///
/// It dereferences to that directory, which also holds the smoke's captures,
/// readbacks, ready file and [`CAGE_LOG_NAME`].
///
/// **Decisions**
///
/// - A fresh mode-0700 directory per cage, created non-recursively so a name
///   collision fails at construction. It separates each cage's Wayland socket,
///   WebKit proxy sockets and runtime files; shared, two cages contend for one
///   Wayland socket.
/// - What blocks a launch from handing off to the developer's running `wolf-gnome`
///   is [`cage_command`]'s `dbus-run-session`, not this directory. Measured with an
///   `org.wolf.Wolf` owner live on `$XDG_RUNTIME_DIR/bus`: without the
///   `XDG_RUNTIME_DIR` line the window smoke stays green and nothing reaches that
///   bus; without the wrapper as well the launch hands off to the owner and exits 11.
///   The directory is defence in depth: it holds no `bus`, so should the wrapper go,
///   GIO's `$XDG_RUNTIME_DIR/bus` fallback finds nothing to hand off to.
/// - Under `temp_dir()`, not beside the cwd: the typed-`:cd` smoke re-roots the
///   sidebar to the cwd's parent, where a directory appearing mid-run is drawn.
/// - Removed on drop when the smoke passed, which bounds the footprint of a full
///   run; kept when it panicked, with its path and the tail of [`CAGE_LOG_NAME`]
///   printed into the failure output.
/// - [`cage_command`] takes it by reference, so no cage can be spawned without both
///   a slot and a private directory.
///
/// **Gotcha:** bind it to a named variable for the whole body. `let _ =` drops it at
/// once, freeing the slot and deleting the directory under the running cage.
struct CageRuntime<'a> {
    dir: PathBuf,
    _slot: CageSlotGuard<'a>,
}

impl<'a> CageRuntime<'a> {
    /// Creates the directory and takes ownership of `slot`. Panics if the path is
    /// longer than [`RUNTIME_DIR_MAX_BYTES`] or already exists.
    fn fresh(slot: CageSlotGuard<'a>) -> Self {
        use std::os::unix::fs::DirBuilderExt;

        let seq = SMOKE_RUNTIME_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("{RUNTIME_DIR_PREFIX}{}-{seq}", std::process::id()));
        let len = dir.as_os_str().len();
        assert!(
            len <= RUNTIME_DIR_MAX_BYTES,
            "the cage runtime directory {} is {len} bytes, over RUNTIME_DIR_MAX_BYTES \
             ({RUNTIME_DIR_MAX_BYTES}), so the cage's Wayland socket under it would not fit a \
             108-byte sun_path. It lives under TMPDIR ({}); point TMPDIR somewhere shorter.",
            dir.display(),
            std::env::temp_dir().display()
        );
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .unwrap_or_else(|e| {
                panic!(
                    "failed to create the cage runtime directory {}: {e}. It must be new: a \
                     shared runtime directory is a shared Wayland socket and session-bus fallback.",
                    dir.display()
                )
            });
        Self { dir, _slot: slot }
    }
}

impl std::ops::Deref for CageRuntime<'_> {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.dir
    }
}

impl Drop for CageRuntime<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.dir);
            return;
        }
        let log_path = self.dir.join(CAGE_LOG_NAME);
        let tail = match std::fs::read_to_string(&log_path) {
            Ok(log) => {
                let lines: Vec<&str> = log.lines().collect();
                lines[lines.len().saturating_sub(CAGE_LOG_TAIL_LINES)..].join("\n")
            }
            Err(e) => format!("<could not read {}: {e}>", log_path.display()),
        };
        eprintln!(
            "cage runtime directory kept for post-mortem: {}\n\
             last {CAGE_LOG_TAIL_LINES} lines of {}:\n{tail}",
            self.dir.display(),
            log_path.display()
        );
    }
}

/// The session-bus wrapper [`cage_command`] runs every scenario under, by the name it
/// is spawned with and looked up on `PATH`.
const DBUS_RUN_SESSION_BIN: &str = "dbus-run-session";

/// The `dbus-run-session` flag that names the bus daemon's configuration file, in the
/// one-argument `--config-file=FILENAME` form its manual gives.
const DBUS_CONFIG_FILE_FLAG: &str = "--config-file=";

/// The name [`smoke_bus_config`] gives the configuration it writes into each
/// [`CageRuntime`] directory.
///
/// It is not `bus`, which is the name GIO's `$XDG_RUNTIME_DIR/bus` fallback looks for
/// and the one entry [`every_cage_gets_a_private_runtime_dir_and_no_host_session`]'s
/// check 4 forbids in that directory.
const SMOKE_BUS_CONFIG_NAME: &str = "smoke-bus.conf";

/// The token [`SMOKE_BUS_CONFIG_TEMPLATE`] carries where the runtime directory goes.
const SMOKE_BUS_LISTEN_DIR_TOKEN: &str = "@LISTEN_DIR@";

/// The configuration every cage's session-bus daemon reads, with
/// [`SMOKE_BUS_LISTEN_DIR_TOKEN`] standing for that cage's runtime directory.
///
/// It is the stock session configuration's semantics minus everything that reaches
/// outside the cage: a session bus that keeps the caller's umask, authenticates by
/// `EXTERNAL` (the peer's uid over the socket) and lets every connection own, send and
/// eavesdrop, which is what a single-user session bus allows. It listens in the
/// cage's own runtime directory, so the socket is private to the cage and removed with
/// it; `dbus-daemon` names it `dbus-` plus ten characters, so the path stays inside the
/// 108-byte `sun_path` that [`RUNTIME_DIR_MAX_BYTES`] budgets for.
///
/// **Decisions**
///
/// - No `<servicedir>`, no `<standard_session_servicedirs/>`, and no `<include>` or
///   `<includedir>` that could bring either back. With no service directory nothing on
///   the bus is activatable: no desktop portal, no dconf writer, no accessibility bus.
///   `<standard_session_servicedirs/>` expands the inherited `XDG_DATA_DIRS`, which on
///   a NixOS host reaches `/run/current-system/sw/share`, and through it a cage's
///   private bus activated the HOST's `xdg-desktop-portal`. That portal masked a real
///   defect, measured: with no portal reachable GTK leaves `gtk-xft-dpi` unset, and a
///   preview web view created after launch on a host not yet allocated lays its page
///   out at a 0×0 viewport and draws blank for good. Forcing `GDK_DEBUG=no-portals` on
///   the host reproduced it exactly, and a container with no portal showed it on every
///   run. A user on a minimal desktop has no portal either, so every smoke runs in that
///   configuration; [`SMOKE_GTK_SETTINGS_INI`] supplies what a portal would otherwise
///   have, except the dpi.
/// - Generated per cage rather than taken from an installation. An installed
///   `session.conf` carries the service directories above, and the host's
///   `/etc/dbus-1/session.conf`, which the daemon reads when no file is named, does not
///   exist in a container built from the flake, where `dbus-run-session` then exits 127
///   before the scenario's first line.
///
/// [`every_cage_gets_a_private_runtime_dir_and_no_host_session`] checks 11 and 12 pin
/// where the file lives and what it may not contain, and
/// `preview_created_after_launch_renders_without_a_portal_under_cage` asks a live
/// daemon what it can activate.
const SMOKE_BUS_CONFIG_TEMPLATE: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:dir=@LISTEN_DIR@</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

/// The GSettings backend every smoke's app is launched with, pinned rather than
/// inherited: `memory`, which reads no settings database and writes nowhere.
///
/// ## Why a pin
///
/// GIO's default backend is dconf, which reads the user's database at
/// `$XDG_CONFIG_HOME/dconf/user` and writes through the `ca.desrt.dconf` service on
/// the session bus. The cage's bus activates nothing (see
/// [`SMOKE_BUS_CONFIG_TEMPLATE`]), so a write could only fail, and where GIO has no
/// dconf module at all it falls back to `memory` by itself and logs that it did.
/// Leaving the variable unset makes the backend a property of the host.
///
/// ## What the pin does NOT buy
///
/// It does not pin what GTK renders with. Measured on a cage's bus, which offers no
/// portal, GTK leaves its font and xft settings unset rather than taking them from
/// GSettings; [`SMOKE_GTK_SETTINGS_INI`] is what pins those. No smoke is evidence about
/// a desktop's live settings or about settings persisting. [`SMOKE_GTK_IM_MODULE`] is
/// the same species of pin.
const SMOKE_GSETTINGS_BACKEND: &str = "memory";

/// The directory inside each [`CageRuntime`] that [`cage_command`] hands the cage as its
/// only `XDG_CONFIG_DIRS` entry. [`smoke_gtk_config_dirs`] writes
/// [`SMOKE_GTK_SETTINGS_INI`] under it.
const SMOKE_XDG_CONFIG_DIR_NAME: &str = "xdg-config";

/// Where GTK 4 looks for a settings file under each `XDG_CONFIG_DIRS` entry.
const GTK_SETTINGS_RELATIVE: &str = "gtk-4.0/settings.ini";

/// The GTK settings every smoke's app starts from, read by GTK from
/// [`GTK_SETTINGS_RELATIVE`] under the cage's `XDG_CONFIG_DIRS`.
///
/// ## Why a pin
///
/// On a desktop GTK takes these from the portal's Settings interface. A cage's bus
/// offers no portal, and measured there GTK leaves them unset: xft antialiasing and
/// hinting at -1, no hint style or subpixel order, and the font `Sans 10`. What it
/// draws then depends on those defaults, which is host-dependent and visibly so —
/// `file_picker_covers_the_live_preview_under_cage`'s assertion 4 read 27 pixels of
/// glyph residue inside its reference band that vanish with these keys pinned. The
/// values are the `org.gnome.desktop.interface` defaults from gsettings-desktop-schemas;
/// `Adwaita Sans` is not among the pinned fonts, so the pinned fontconfig's last-resort
/// rule resolves it to DejaVu Sans.
///
/// **Decisions**
///
/// - It deliberately does NOT set `gtk-xft-dpi`. The app pins that itself in
///   `crates/wolf-gnome/src/app.rs` when it is unset, because without it WebKitGTK lays
///   a view created after launch out at a 0×0 page viewport; a dpi here would satisfy
///   the pin's condition, and every preview smoke would pass with the pin deleted.
///   [`every_cage_gets_a_private_runtime_dir_and_no_host_session`] check 15 refuses one.
/// - Through `XDG_CONFIG_DIRS`, which REPLACES the host's system directories, so a
///   `gtk-4.0/settings.ini` under the host's `/etc/xdg` cannot reach a smoke. GTK lets a
///   user-level file override the system ones, which is why [`cage_command`] also points
///   `XDG_CONFIG_HOME` at the smoke home's empty `.config`. GTK's compiled-in
///   sysconfdir file is the one source this does not replace; `DPI_PIN_STDERR_PREFIX`
///   is what catches a dpi arriving from it.
const SMOKE_GTK_SETTINGS_INI: &str = "\
[Settings]
gtk-font-name=Adwaita Sans 11
gtk-xft-antialias=1
gtk-xft-hinting=1
gtk-xft-hintstyle=hintslight
gtk-xft-rgba=none
";

/// Whether `byte` may appear unescaped in a D-Bus address value: the spec's
/// optionally-escaped set, `[-0-9A-Za-z_/.\*]`.
///
/// None of them is special to XML either, so a directory made only of these can be
/// written into [`SMOKE_BUS_CONFIG_TEMPLATE`]'s `<listen>` element as it is.
fn is_dbus_address_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-_/.\\*".contains(&byte)
}

/// Writes [`SMOKE_BUS_CONFIG_TEMPLATE`] for the cage whose runtime directory is
/// `runtime` into that directory, as [`SMOKE_BUS_CONFIG_NAME`], and returns its path.
/// [`cage_command`] hands the path to `dbus-run-session`.
///
/// Panics when the directory holds a byte a D-Bus address would need escaped, which
/// can only come from `TMPDIR`; the message says so rather than writing an address the
/// daemon would misread. Each call rewrites the file, so building several commands
/// over one runtime is harmless.
fn smoke_bus_config(runtime: &Path) -> PathBuf {
    let dir = runtime
        .to_str()
        .filter(|dir| dir.bytes().all(is_dbus_address_safe))
        .unwrap_or_else(|| {
            panic!(
                "the cage runtime directory {} holds a byte outside the D-Bus address set \
                 [-0-9A-Za-z_/.\\*], so it cannot be written unescaped into the session bus's \
                 `<listen>unix:dir=…</listen>`. It lives under TMPDIR ({}); point TMPDIR at a \
                 plain path.",
                runtime.display(),
                std::env::temp_dir().display()
            )
        });
    let path = runtime.join(SMOKE_BUS_CONFIG_NAME);
    let text = SMOKE_BUS_CONFIG_TEMPLATE.replace(SMOKE_BUS_LISTEN_DIR_TOKEN, dir);
    std::fs::write(&path, text).unwrap_or_else(|e| {
        panic!(
            "failed to write the cage's session-bus configuration {}: {e}",
            path.display()
        )
    });
    path
}

/// Writes [`SMOKE_GTK_SETTINGS_INI`] at [`GTK_SETTINGS_RELATIVE`] under
/// [`SMOKE_XDG_CONFIG_DIR_NAME`] inside `runtime`, and returns that directory, which
/// [`cage_command`] hands the cage as `XDG_CONFIG_DIRS`. Each call rewrites the file.
fn smoke_gtk_config_dirs(runtime: &Path) -> PathBuf {
    let dir = runtime.join(SMOKE_XDG_CONFIG_DIR_NAME);
    let file = dir.join(GTK_SETTINGS_RELATIVE);
    let parent = file
        .parent()
        .expect("GTK_SETTINGS_RELATIVE names a file inside a directory");
    std::fs::create_dir_all(parent).unwrap_or_else(|e| {
        panic!(
            "failed to create the cage's GTK settings directory {}: {e}",
            parent.display()
        )
    });
    std::fs::write(&file, SMOKE_GTK_SETTINGS_INI).unwrap_or_else(|e| {
        panic!(
            "failed to write the cage's GTK settings {}: {e}",
            file.display()
        )
    });
    dir
}

/// Builds the `cage` invocation every smoke runs its scenario through — and is the
/// SINGLE place in this crate's tests where the compositor is ever spawned.
///
/// It fixes the invocation (`cage -- dbus-run-session --config-file=<smoke-bus.conf>
/// -- sh -c <READINESS_PRELUDE><scenario>`, with `cwd`
/// as the working directory, which is how the app comes to derive `CoreConfig.cwd`
/// from it), its stdout and stderr (appended to the runtime's [`CAGE_LOG_NAME`], so
/// parallel cages do not interleave), the environment variables EVERY smoke
/// must have, and the ones it must NOT have:
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
/// * `WOLF_CONFIG_DIR` = [`smoke_config_dir`] — the isolation that keeps a smoke
///   run out of the developer's real `~/.config/wolf/`, and the thing that pins the
///   app's font family and line-height to what [`cell_metrics_for_smoke`] measures.
///   Read that function's doc comment before changing anything here. Without it
///   `preferences_dir()` falls through to `XDG_CONFIG_HOME`, and on ANY host
///   `load_or_create` CREATES an absent file carrying `default_config()`: family
///   `iA Writer Mono S`, which the pinned `FONTCONFIG_FILE` resolves, so every glyph
///   in every OCR smoke would change shape.
/// * `HOME` = [`smoke_home`] — the cwd's PARENT, which is what pins the rendered
///   window title to the constant `~/project`. See below; this one is an OCR
///   DETERMINISM control, not tidiness.
/// * `XDG_CACHE_HOME` = [`smoke_shader_cache_dir`] — ONE directory shared by every
///   smoke of a `cargo test` invocation, so mesa's shader cache is warm for all of
///   them but the first. It does NOT carry into the next invocation: the path is
///   under `TMPDIR`, which `nix develop --command` rewrites every time (measured; see
///   that function). Of the paths set here it is the only one that is
///   deliberately NOT per-smoke, and it is the direct counterweight to the `HOME` line above:
///   redirecting `HOME` also redirected `$HOME/.cache`, which made every launch
///   rebuild ~4.9 MB of shaders. A cold cache costs launch time spent in `wait_ready`,
///   not correctness. Read that function's doc comment — including why sharing
///   carries no state between smokes — before changing it, and do not make it
///   per-smoke.
/// * `GTK_IM_MODULE` = [`SMOKE_GTK_IM_MODULE`] — which input method every smoke's
///   typing goes through. `gtk::IMMulticontext` selects its slave from
///   `GtkSettings:gtk-im-module`, which GTK initializes from this variable, and this
///   function inherits the developer's environment wholesale. Read that const's doc
///   comment before changing it: the pin is what stops the suite's typing becoming
///   host-dependent, and it does NOT make any smoke evidence about an
///   out-of-process input method.
/// * `GSETTINGS_BACKEND` = [`SMOKE_GSETTINGS_BACKEND`] — so the settings backend is
///   the same on every host and never reaches for a dconf writer the cage's bus cannot
///   activate. Read that const's doc comment before changing it.
/// * `XDG_CONFIG_DIRS` = [`smoke_gtk_config_dirs`] — a directory inside the runtime
///   directory holding [`SMOKE_GTK_SETTINGS_INI`], the font and xft settings a portal
///   would otherwise supply, and NOT the dpi. Read that const's doc comment before
///   changing it: a dpi there masks the app's own pin, which the preview smokes exist
///   to test.
/// * `XDG_CONFIG_HOME` = the `.config` directory under [`smoke_home`], which is where
///   it defaults to anyway and where nothing exists. Inherited, a host exporting it
///   hands GTK the developer's own `gtk-4.0/settings.ini`, which overrides the pinned
///   one — `gtk-xft-dpi` included. It is also exactly the directory
///   `HOME_CONFIG_DIR_FALLBACK` is relative to, so a config leak still lands where
///   `single_pane_file_survives_relaunch_under_cage` looks for it.
/// * `XDG_RUNTIME_DIR` = the [`CageRuntime`]'s private directory, where the cage
///   makes its Wayland socket and WebKit its proxy sockets. It holds no `bus`, which
///   is defence in depth behind `dbus-run-session`, the thing that keeps a launch off
///   the developer's session bus. See [`CageRuntime`].
/// * `WOLF_READY_FILE` = [`READY_FILE_NAME`] inside that directory — where the app
///   writes its pid once its launch windows accept keystrokes, read by the
///   `wait_ready` that [`READINESS_PRELUDE`] defines. The prelude is prepended to
///   every scenario here, which is what lets no scenario carry its own copy and none
///   forget it.
/// * `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY` are REMOVED: each names the
///   developer's own display. libwayland prefers `WAYLAND_SOCKET` over the
///   `WAYLAND_DISPLAY` cage sets for its child, wlroots picks a nested backend from
///   `WAYLAND_DISPLAY` wherever `WLR_BACKENDS` is unset, and with `DISPLAY` set GIO
///   can autolaunch a session bus over X11.
/// * `DBUS_SESSION_BUS_ADDRESS` is REMOVED rather than set. **A DANGLING one kills
///   the app before its first frame**:
///   measured under `cage` in three cases, a path that does not exist is FATAL (the
///   proximate message on the measurement host is
///   `bwrap: Can't create file at /.flatpak-info: Permission denied`, from WebKit's
///   mandatory bubblewrap sandbox), while unset is fine and an existing-but-dead
///   socket path is fine. `WEBKIT_FORCE_SANDBOX=0` does not exist in this ABI — the
///   sandbox cannot be turned off — so clearing the variable is the only lever there
///   is. Every smoke inherits the developer's environment except for what this
///   function overrides, so before the `env_remove` the smokes were safe here purely
///   by luck: a developer whose session bus address points at a stale path would see
///   every one of them fail at launch (exit 11), reading as a launch failure in
///   whatever change happened to be in flight.
///
/// ## Why the scenario runs under `dbus-run-session`, on a bus that activates nothing
///
/// It gives each cage exactly one session bus, private to it and gone when it exits.
/// The app takes the address `dbus-run-session` sets and never looks for
/// `$XDG_RUNTIME_DIR/bus`, so this wrapper is what keeps a launch from handing off to
/// the developer's running `wolf-gnome` (measured; see [`CageRuntime`]). Without the
/// wrapper GIO finds no address and autolaunches a bus through `dbus-launch` on the
/// cage's own Xwayland `DISPLAY`, starting an Xwayland and a stray `dbus-daemon` per
/// launch. The `env_remove`s above still apply to the cage itself;
/// `dbus-run-session` sets its own address for `sh`.
///
/// The daemon reads the configuration [`smoke_bus_config`] writes into the runtime
/// directory, never one installed on the host or in the store, and that file names no
/// service directory. The cause is measured: a bus configured from an installed
/// `session.conf` activated the host's desktop portal through the inherited
/// `XDG_DATA_DIRS`, and the portal supplied the `gtk-xft-dpi` without which a preview
/// view created after launch draws blank — so the smokes passed on a NixOS host and
/// failed in every container, as the app does on every minimal desktop.
/// [`SMOKE_BUS_CONFIG_TEMPLATE`] carries the measurement, and
/// [`SMOKE_GTK_SETTINGS_INI`] supplies the portal's other settings.
///
/// The wrapper itself is there for the hand-off isolation above.
///
/// ## Why `HOME` is set, and why it is the cwd's parent
///
/// The `adw::HeaderBar` renders the app's working directory, so the temp
/// directory's name is page text in every capture — and
/// `tesseract` classifies a word by the WHOLE page (see [`SIDEBAR_FILLER_COUNT`]).
/// While the title drew the run-varying `<tmpdir>/wolf-gnome-smoke-<pid>-<nanos>`,
/// the captured page differed on every run, measured:
///
/// | arm | five consecutive capture md5s | size |
/// | --- | --- | --- |
/// | title rendering the run-varying temp path | all five DIFFERENT | 34.2–35.0 KB, varying |
/// | `set_title` suppressed (the base page) | `5ea4d070…` ×5, byte-identical | 26074 ×5 |
///
/// and `window_renders_typed_text_under_cage` failed ~2% of runs (2 in ~93) with
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
/// root) or absent-and-defaulted (GTK finds no `$HOME/.config/gtk-4.0`, so it takes
/// only the system settings `XDG_CONFIG_DIRS` names, [`SMOKE_GTK_SETTINGS_INI`] — the
/// same for every smoke, `empty_state_shows_open_folder_button_under_cage` included,
/// whose `HOME` is the cwd). Fontconfig is
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
/// Some smokes chain their OWN `HOME` after this call and deliberately win, such as
/// `empty_state_shows_open_folder_button_under_cage` (`HOME` == the cwd, to fire
/// the `EmptyState` branch) and
/// `window_title_shows_the_working_directory_under_cage` (`HOME` == its nested
/// project's parent, which is the value its abbreviation assertion is written
/// against). Both still render a CONSTANT title (`~` and `~/titlezulu`
/// respectively), so both keep the determinism property this default exists for.
///
/// Callers chain only their own scenario-specific `.env(...)` / `.arg(...)`
/// additions on top. Two per-smoke font notes live at their call sites rather than
/// on the `WOLF_FONT_PT` line: the vsplit / hsplit / three-pane smokes point at
/// the OCR width and height note in notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes,
/// because if two or three panes are ever too narrow or too short for 28 pt OCR the
/// fix is per-test (widen the window, or lower the font for THAT test) and must not
/// be applied here to every smoke.
///
/// [`cage_is_only_spawned_through_the_isolating_helper`] enforces the "single place"
/// property mechanically, because a smoke that spawned the compositor directly would
/// silently lose every variable above, the readiness prelude and the private runtime
/// directory.
fn cage_command(scenario: &str, cwd: &Path, runtime: &CageRuntime<'_>) -> Command {
    let log_path = runtime.join(CAGE_LOG_NAME);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap_or_else(|e| panic!("failed to open the cage log {}: {e}", log_path.display()));
    let log_err = log
        .try_clone()
        .unwrap_or_else(|e| panic!("failed to clone the cage log {}: {e}", log_path.display()));
    let mut session_conf = std::ffi::OsString::from(DBUS_CONFIG_FILE_FLAG);
    session_conf.push(smoke_bus_config(runtime));
    let mut cmd = Command::new("cage");
    cmd.arg("--")
        // The cage's one private session bus, which dies with it.
        .arg(DBUS_RUN_SESSION_BIN)
        // Generated in the runtime dir: no service dirs, so nothing on the bus is activatable.
        .arg(session_conf)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(format!("{READINESS_PRELUDE}\n{scenario}"))
        .current_dir(cwd)
        .env("HOME", smoke_home(cwd))
        .env("XDG_CACHE_HOME", smoke_shader_cache_dir())
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
        .env("WOLF_STATE_DIR", smoke_state_dir(cwd))
        .env("WOLF_CONFIG_DIR", smoke_config_dir(cwd))
        // Pinned rather than inherited: see `SMOKE_GTK_IM_MODULE`.
        .env("GTK_IM_MODULE", SMOKE_GTK_IM_MODULE)
        // Pinned rather than inherited: see `SMOKE_GSETTINGS_BACKEND`.
        .env("GSETTINGS_BACKEND", SMOKE_GSETTINGS_BACKEND)
        // A portal's font and xft settings, never the dpi: see `SMOKE_GTK_SETTINGS_INI`.
        .env("XDG_CONFIG_DIRS", smoke_gtk_config_dirs(runtime))
        // Its default, pinned: a host's export would let a user settings.ini override the pin.
        .env("XDG_CONFIG_HOME", smoke_home(cwd).join(".config"))
        // The cage's own Wayland socket, and no `bus` for GIO to fall back to.
        .env("XDG_RUNTIME_DIR", &**runtime)
        // Where the app writes its pid once it accepts input; `wait_ready` in
        // `READINESS_PRELUDE` reads it.
        .env("WOLF_READY_FILE", runtime.join(READY_FILE_NAME))
        // Removed, never set: a dangling session-bus address kills the app under WebKit's sandbox.
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        // The developer's display, which nothing inside the cage may reach.
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .stdout(log)
        .stderr(log_err);
    cmd
}

/// One Rust source file of this crate's tests, as the source-scanning guards read it.
struct TestSource {
    /// The path relative to the crate's `tests/`, with `/` separators, such as
    /// `smoke_headless/main.rs`.
    path: String,
    text: String,
}

/// This binary's crate root, relative to the crate's `tests/`.
const SMOKE_MAIN: &str = "smoke_headless/main.rs";

/// Every `*.rs` file under `tests/<dir>` of this crate (`""` names `tests/` itself),
/// recursively and sorted by path, read at run time rather than through `include_str!`,
/// so a guard over this binary sees every module file.
///
/// Panics, naming the directory or file, when one cannot be read: an empty or partial
/// list would leave a guard that scans it checking less than it claims.
fn read_test_sources(dir: &str) -> Vec<TestSource> {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut pending = vec![tests.join(dir)];
    let mut sources = Vec::new();
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current).unwrap_or_else(|e| {
            panic!(
                "cannot read the test sources under {}: {e}. A source-scanning guard that read \
                 nothing would check nothing, so it fails instead.",
                current.display()
            )
        });
        for entry in entries {
            let path = entry
                .unwrap_or_else(|e| panic!("cannot list {}: {e}", current.display()))
                .path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension() == Some(OsStr::new("rs")) {
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                let relative = path
                    .strip_prefix(&tests)
                    .unwrap_or_else(|e| panic!("{} is not under tests/: {e}", path.display()))
                    .to_string_lossy()
                    .replace('\\', "/");
                sources.push(TestSource {
                    path: relative,
                    text,
                });
            }
        }
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources
}

/// Asserts that `sources` holds [`SMOKE_MAIN`] and, for every `mod <name>;` line in it,
/// `smoke_headless/<name>.rs`, so `guard` cannot pass by scanning fewer files than the
/// binary compiles.
fn assert_reads_every_module(guard: &str, sources: &[TestSource]) {
    let main = sources
        .iter()
        .find(|source| source.path == SMOKE_MAIN)
        .unwrap_or_else(|| {
            panic!(
                "{guard}: {SMOKE_MAIN} is not among the files it read ({:?}), so it is not \
                 scanning this binary at all and would pass over any source. Fix \
                 `read_test_sources` or the directory it is given.",
                sources.iter().map(|s| s.path.as_str()).collect::<Vec<_>>()
            )
        });
    let declared: Vec<String> = main
        .text
        .lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix("mod ")?.strip_suffix(';')?;
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                .then(|| format!("smoke_headless/{name}.rs"))
        })
        .collect();
    assert!(
        !declared.is_empty(),
        "{guard}: found no `mod <name>;` line in {SMOKE_MAIN}. This binary is split into module \
         files, so either the declarations were reformatted out of this check's reach or it is \
         reading the wrong file; either way it cannot tell whether every module was scanned."
    );
    let missing: Vec<&String> = declared
        .iter()
        .filter(|path| !sources.iter().any(|source| &source.path == *path))
        .collect();
    assert!(
        missing.is_empty(),
        "{guard}: {SMOKE_MAIN} declares modules whose files it did not read: {missing:?}. A \
         module compiled into this binary but not scanned is a blind spot: whatever this guard \
         forbids could sit there and stay green."
    );
}

/// The source text that spawns the compositor, assembled from THREE FRAGMENTS so
/// that no test source contains it contiguously anywhere except inside
/// [`cage_command`].
///
/// The fragment construction is load-bearing, not stylistic — see
/// [`cage_is_only_spawned_through_the_isolating_helper`], which scans the crate's test
/// sources for it. Written as one contiguous literal it would match ITSELF, the count
/// would be two, and the guard would fail permanently no matter how correct the code
/// was. Do not "simplify" it.
const CAGE_SPAWN_NEEDLE: &str = concat!("Command::new(\"", "cage", "\")");

/// Enforces mechanically that the compositor is spawned in exactly ONE place among
/// this crate's test sources — [`cage_command`] in this binary's `main.rs` — which is
/// the only thing that sets `WOLF_STATE_DIR`,
/// `HOME`, `XDG_CACHE_HOME`, `GTK_IM_MODULE`, `GSETTINGS_BACKEND`, `XDG_CONFIG_DIRS`,
/// `XDG_CONFIG_HOME`, `WOLF_READY_FILE` and the private `XDG_RUNTIME_DIR`, the only
/// thing that prepends
/// [`READINESS_PRELUDE`], and the only thing that CLEARS `DBUS_SESSION_BUS_ADDRESS`
/// and the host display variables.
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
/// And a third, a cost rather than a flake: without [`cage_command`]'s
/// `XDG_CACHE_HOME` the app's mesa shader cache is whatever the inherited environment
/// gives it, which under a redirected `HOME` is a cold per-run directory. That costs
/// ~4.9 MB of shader rebuild on every launch, paid as time inside `wait_ready`. See
/// [`smoke_shader_cache_dir`].
///
/// A fourth, and it is the loudest of the set: without `WOLF_CONFIG_DIR` the app
/// resolves its preferences directory from `XDG_CONFIG_HOME` — which the `HOME`
/// redirect does not cover — or from `$HOME/.config`, where no file exists, at which
/// point `load_or_create` WRITES one carrying `default_config()`. The family in it is
/// `iA Writer Mono S`, which the pinned `FONTCONFIG_FILE` resolves, so that smoke
/// renders in a different typeface from the one [`cell_metrics_for_smoke`] measured
/// and every OCR term and every pointer target in it is computed against the wrong
/// cell. See [`smoke_config_dir`].
///
/// A fifth, and it is the one that would be least obvious in a red run because it
/// depends on the MACHINE. Without [`cage_command`]'s `GTK_IM_MODULE` the app takes
/// whichever input method the developer's environment names, and that context is
/// what every printable keystroke travels through — the window offers each press to
/// it before translating. On a host carrying the GTK 4 ibus module the typing in
/// that smoke reroutes through an engine whose daemon is not on the cage's private
/// bus, so keys are swallowed on that machine and nowhere else. See [`SMOKE_GTK_IM_MODULE`] for the
/// measurement and for why the value is named rather than the variable cleared.
///
/// A sixth, which is a REMOVAL rather than a setting and is the only one of the
/// eight that can take the app down before it draws anything at all. A smoke that
/// spawns `cage` itself inherits the developer's `DBUS_SESSION_BUS_ADDRESS`, and a
/// DANGLING one — a path that does not exist — is fatal under WebKit's mandatory
/// bubblewrap sandbox (measured: non-existent path fatal, unset fine,
/// existing-but-dead fine). The app dies before its first frame, so the scenario's
/// readiness wait reports `exit 11` and the run reads as a LAUNCH failure in whatever
/// change is in flight, on the developer's machine and nowhere else. See
/// [`cage_command`]'s bullet for the measurement and for why the sandbox cannot
/// simply be turned off.
///
/// A seventh, which fails loudly on every machine: the scenario loses
/// [`READINESS_PRELUDE`] and `WOLF_READY_FILE` together, so its first `wait_ready`
/// call exits 127 (no such command) — or, were the prelude copied in, `set -u` stops
/// it at the unset `WOLF_READY_FILE`.
///
/// And an eighth, which passes where it should fail: without the `dbus-run-session`
/// wrapper the scenario has no session bus of its own, so GIO finds the developer's
/// at `$XDG_RUNTIME_DIR/bus` in the inherited runtime directory, the launch hands off
/// to their running `wolf-gnome`, and the smoke drives that process instead of the one
/// under test. The wrapper is what blocks that hand-off. Losing the private
/// `XDG_RUNTIME_DIR` as well puts the cage's Wayland socket, WebKit proxy sockets and
/// runtime files in the developer's directory beside every other parallel cage.
///
/// ## Why the needle is built from fragments, and why that is load-bearing
///
/// This test reads every `*.rs` under the crate's `tests/` at run time through
/// [`read_test_sources`]: every module of this binary and every other test binary of
/// the crate, so a spawn in a second file is counted too. It asserts that it read
/// each module [`SMOKE_MAIN`] declares. Were [`CAGE_SPAWN_NEEDLE`]
/// spelled as one contiguous literal, the const's own line would contain it, the
/// count would be two, and the test would be red forever. The tempting "fix" for a
/// permanently-red count is to relax it to `<= 1` — at which point a needle that
/// matches NOTHING (someone renames the binary, or `cage` moves behind a const)
/// satisfies it vacuously and the guard silently stops guarding. That is exactly the
/// vacuity shape `crates/wolf-gnome/tests/README.md` records under "Smoke
/// conventions" ("A red-run demonstration is a deliverable, not a formality"): a
/// check aimed at the right property, looking somewhere the defect cannot appear.
/// So the count is asserted from BOTH sides: at least one (the guard is not vacuous)
/// and at most one (the invariant holds).
///
/// This test is PURE: no compositor, no `require_smoke_env`, no cage slot, no
/// subprocess — the same shape as
/// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion`, and it runs in
/// microseconds on a machine with no smoke toolchain at all.
#[test]
fn cage_is_only_spawned_through_the_isolating_helper() {
    let sources = read_test_sources("");
    assert_reads_every_module(
        "cage_is_only_spawned_through_the_isolating_helper",
        &sources,
    );
    let holders: Vec<(&str, usize)> = sources
        .iter()
        .map(|source| {
            (
                source.path.as_str(),
                source.text.matches(CAGE_SPAWN_NEEDLE).count(),
            )
        })
        .filter(|&(_, count)| count > 0)
        .collect();
    let occurrences: usize = holders.iter().map(|&(_, count)| count).sum();

    assert!(
        occurrences > 0,
        "the compositor-spawn needle {CAGE_SPAWN_NEEDLE:?} does not appear in any of the crate's \
         test sources AT ALL, so this guard is VACUOUS: it would accept any number of unguarded \
         spawns because it can no longer recognize one. Either the spawn in `cage_command` was \
         reworded (a binary name moved behind a const, a builder replaced the constructor, \
         rustfmt split the call across lines), or `read_test_sources` is no longer reading \
         {SMOKE_MAIN}. Fix the needle or the helper so the two agree again — do NOT relax the \
         exactly-once check below to an upper bound, which is what would let this zero-match \
         state pass."
    );
    assert!(
        occurrences == 1 && holders[0].0 == SMOKE_MAIN,
        "expected exactly ONE occurrence of {CAGE_SPAWN_NEEDLE:?} among the crate's test sources, \
         the one inside `cage_command` in {SMOKE_MAIN}; found {occurrences}, in (file, count) \
         {holders:?}. Every extra one is a compositor spawned OUTSIDE \
         `cage_command` and therefore without `WOLF_STATE_DIR`, `WOLF_CONFIG_DIR`, `HOME`, \
         `XDG_CACHE_HOME`, `GTK_IM_MODULE`, `WOLF_READY_FILE`, the readiness prelude, the \
         `dbus-run-session` wrapper or its private `XDG_RUNTIME_DIR`, and WITH whatever \
         `DBUS_SESSION_BUS_ADDRESS` and display the developer's session exported. Without the \
         wrapper there is no private session bus, so GIO finds the developer's at \
         `$XDG_RUNTIME_DIR/bus`, the launch hands off to their running wolf-gnome and the smoke \
         drives the wrong process; without the private directory the cage's Wayland socket, \
         WebKit proxy sockets and runtime files land in the developer's runtime directory \
         beside every other parallel cage. Five more of those losses are spelled out here: one is loud \
         on every machine, one is fatal, two are destructive and one is host-dependent. Losing the prelude is the loud one: its \
         `wait_ready` call exits 127 (or, with the prelude copied in, `set -u` stops at the \
         unset `WOLF_READY_FILE`). A DANGLING session-bus address is the fatal one: it kills the \
         app before its first frame under WebKit's mandatory sandbox, so that smoke exits 11 \
         and reads as a launch failure in whatever change is in flight. Without \
         `WOLF_STATE_DIR` that smoke's app resolves `state_dir()` to the developer's real \
         `~/.local/state/wolf/`, READS their real state.json at launch (so it starts from the developer's last session instead of a clean \
         slate) and OVERWRITES it on quit with its own throwaway temp-cwd layout. Without \
         `WOLF_CONFIG_DIR` it resolves `preferences_dir()` from `XDG_CONFIG_HOME` or \
         `$HOME/.config` and `load_or_create` READS the developer's real wolf-config.yaml — or, \
         where none exists, CREATES one carrying `default_config()`, whose family \
         `iA Writer Mono S` the pinned FONTCONFIG_FILE resolves, so that smoke draws every glyph \
         in a different typeface from the one `cell_metrics_for_smoke` measured. Without \
         `GTK_IM_MODULE` it takes whichever input method the developer's environment names, and \
         every printable keystroke is offered to that context before it is translated — so on a \
         host carrying the GTK 4 ibus module the typing reroutes through an engine whose session \
         bus this harness removes, and the smoke fails on that machine and nowhere else. Route \
         the new smoke through `cage_command(scenario, &cwd, &runtime_dir)` and chain only its own `.env(...)` \
         additions on top; do not copy the helper's body."
    );
}

// ---------------------------------------------------------------------------
// Guards over the cage slots and the private runtime directory each cage runs in.

/// Every cage runs in a private runtime directory of its own, nothing in its
/// environment names the developer's session bus or display, its own bus can activate
/// no service, and GTK reads its settings from the cage alone.
///
/// ## What it closes
///
/// A launch that reaches the developer's session bus hands off to their running
/// `wolf-gnome` through single-instance activation. The smoke then drives the wrong
/// process, and on a host with no `wolf-gnome` running it passes anyway, so a green
/// run elsewhere proves nothing. The `dbus-run-session` wrapper (check 10) is what
/// blocks that hand-off: it gives each cage a private session bus that dies with it.
/// Measured with an `org.wolf.Wolf` owner live on the bus a cage would otherwise
/// find: deleting only the `XDG_RUNTIME_DIR` line stays green with nothing reaching
/// that bus, while removing the wrapper as well brings the full hand-off back.
///
/// The private runtime directory (checks 1 to 5) does a different job. Several cages
/// run at once ([`CageSlots`]), and a directory two of them share holds one set of
/// Wayland sockets, WebKit proxy sockets and runtime files for both. It is also
/// defence in depth should the wrapper go: GIO's fallback without an address is
/// `$XDG_RUNTIME_DIR/bus`, and the private directory offers none. Every property
/// below is one `.env` line, one argument or one `DirBuilder` argument, far from any
/// smoke that depends on it.
///
/// ## What it checks
///
/// It builds two [`CageRuntime`]s on a local [`CageSlots`] and reads, through
/// `Command::get_envs`, the commands [`cage_command`] and [`packaged_cage_command`]
/// build over them. There an `.env` shows as `(name, Some(value))`, an `.env_remove`
/// as `(name, None)`, and an inherited variable not at all. Each falsifying edit
/// below fires its own check and no earlier one.
///
/// - **Non-vacuity:** `HOME`, `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR` are set, so the
///   map really is the one [`cage_command`] builds.
/// 1. `XDG_RUNTIME_DIR` is set to runtime A's directory, which differs from this
///    process's own value. Falsify: delete `.env("XDG_RUNTIME_DIR", …)` from
///    [`cage_command`].
/// 2. Runtimes A and B have different directories, and B's command carries B's.
///    Falsify: drop the sequence number from [`CageRuntime::fresh`]'s directory
///    name. `fresh` itself then fails with `AlreadyExists` building B, which is the
///    intended first line; adding `.recursive(true)` to its `DirBuilder` as well
///    reaches this check's own message.
/// 3. Each directory is a real directory (not a symlink), mode `0700`, owned by the
///    temp cwd's owner. Falsify: `.mode(0o755)` in [`CageRuntime::fresh`].
/// 4. Neither directory has a `bus` entry after its command is built. Falsify: in
///    [`CageRuntime::fresh`], `std::fs::File::create(dir.join("bus"))`, or a symlink
///    there to the developer's `/run/user/<uid>/bus`.
/// 5. Each path is at most [`RUNTIME_DIR_MAX_BYTES`]. Falsify: set that const to
///    `10`. `fresh`'s own length assertion fires first; with it deleted, this one
///    does.
/// 6. `DBUS_SESSION_BUS_ADDRESS` is removed. Falsify: delete its `.env_remove`.
/// 7. `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY` are each removed. Falsify:
///    delete any one of the three `.env_remove`s; all missing ones are reported.
/// 8. `WOLF_READY_FILE` lies inside its own runtime's directory, for A and B.
///    Falsify: set it to `std::env::temp_dir().join(READY_FILE_NAME)`.
/// 9. Both [`packaged_cage_command`] arms, [`GtkRuntimeEnv::Scrubbed`] and
///    [`GtkRuntimeEnv::Inherited`], keep checks 1, 6 and 7. Falsify: add
///    `"XDG_RUNTIME_DIR"` to [`PACKAGED_SCRUB_ALWAYS`].
/// 10. [`cage_command`] over A and over B, and both [`packaged_cage_command`] arms,
///     pass `cage` exactly `--`, `dbus-run-session`, one `--config-file=<path>`, `--`,
///     `sh`, `-c` and then one argument, the script, so every scenario runs under a
///     private session bus that dies with its cage. Without the wrapper GIO
///     autolaunches a bus through the cage's own Xwayland `DISPLAY`. Falsify: drop
///     `dbus-run-session` and its `--` from [`cage_command`]; separately, drop the
///     `.arg(session_conf)` line, which passes on a host with `/etc/dbus-1` and exits
///     127 in a container without it.
/// 11. Each `<path>` is absolute, a regular file rather than a symlink, and a direct
///     child of the runtime directory its command was built over: A's in A, B's in B.
///     Falsify: make [`smoke_bus_config`] return the `share/dbus-1/session.conf`
///     installed beside `dbus-run-session`; separately, write the file under
///     `std::env::temp_dir()` instead of the runtime directory.
/// 12. Each file's text listens in its own runtime directory, which also says it is
///     the generated configuration, and contains no `<servicedir`, no
///     `<standard_session_servicedirs` and no `<include`. Falsify: add
///     `<standard_session_servicedirs/>` to [`SMOKE_BUS_CONFIG_TEMPLATE`].
/// 13. `GSETTINGS_BACKEND` is set to `memory` on [`cage_command`] and on both
///     [`packaged_cage_command`] arms. Falsify: delete its `.env` line from
///     [`cage_command`]; separately, add it to [`PACKAGED_SCRUB_ALWAYS`].
/// 14. `XDG_CONFIG_DIRS` is set, on all four commands, to one absolute directory
///     directly inside the runtime directory the command was built over, holding a
///     regular `gtk-4.0/settings.ini`. Falsify: delete its `.env` line from
///     [`cage_command`]; separately, make [`smoke_gtk_config_dirs`] write under
///     `std::env::temp_dir()`.
/// 15. That file's `[Settings]` section holds exactly the five font and xft keys of
///     [`SMOKE_GTK_SETTINGS_INI`], re-typed here, and nothing outside it; in
///     particular it does not set `gtk-xft-dpi`. Falsify: add `gtk-xft-dpi=98304` to
///     [`SMOKE_GTK_SETTINGS_INI`] (the message names the masked pin); separately,
///     delete its `gtk-font-name` line.
/// 16. `XDG_CONFIG_HOME` is set to the `.config` directory under the `HOME` the same
///     command sets. Falsify: delete its `.env` line from [`cage_command`].
///
/// ## The premise, and what it does not cover
///
/// `get_envs` and `get_args` report what the builder applies at spawn, which is what
/// std documents for them. The guard spawns nothing, so what `cage` itself exports to
/// its child (for example a `DISPLAY` from Xwayland) is outside its reach and is
/// measured instead; check 10 pins the wrapper that gives that child its own bus
/// whatever `cage` exports.
///
/// Check 12 reads the configuration as text and parses no XML, so it says what the
/// file names and not what a daemon does with it.
/// `preview_created_after_launch_renders_without_a_portal_under_cage` asks a live
/// daemon started from it what it can activate. Check 15 likewise says what GTK is
/// handed, not what it applies. A dpi reaching GTK some other way — its compiled-in
/// sysconfdir `gtk-4.0/settings.ini`, which `XDG_CONFIG_DIRS` does not replace — masks
/// the app's pin and leaves every preview smoke GREEN, so no pixel can catch it; what
/// does is that smoke and the markdown-preview smoke asserting their cage log carries
/// the pin's `DPI_PIN_STDERR_PREFIX` line.
///
/// A semaphore broken so that two slots cannot be held at once hangs this guard in
/// `acquire`; [`cage_slots_bound_concurrency`] is the guard that turns that into a
/// failure.
///
/// ## What it needs
///
/// No compositor, no [`require_smoke_env`], no global cage slot, no subprocess and no
/// `PATH` lookup. It writes a temp cwd, two runtime directories and the bus and GTK
/// configuration [`cage_command`] writes into each under `temp_dir()`; the runtimes
/// remove theirs when dropped and the guard removes the cwd's parent, so a passing run
/// leaves nothing behind.
#[test]
fn every_cage_gets_a_private_runtime_dir_and_no_host_session() {
    const GUARD: &str = "every_cage_gets_a_private_runtime_dir_and_no_host_session";
    // Re-typed rather than shared with `cage_command`, so renaming a variable there
    // fails here instead of moving the check along with it.
    const RUNTIME_VAR: &str = "XDG_RUNTIME_DIR";
    const READY_VAR: &str = "WOLF_READY_FILE";
    const BUS_VAR: &str = "DBUS_SESSION_BUS_ADDRESS";
    const DISPLAY_VARS: [&str; 3] = ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"];
    const ISOLATION_VARS: [&str; 3] = ["HOME", "WOLF_STATE_DIR", "WOLF_CONFIG_DIR"];
    // GIO's session-bus fallback, relative to `XDG_RUNTIME_DIR`.
    const GIO_BUS_FALLBACK: &str = "bus";
    // Room for both runtimes below to hold a slot at once.
    const LOCAL_LIMIT: usize = 2;
    // What `cage` must run, ahead of the script: a private session bus around `sh`,
    // with the flag naming its configuration between the two halves.
    const BUS_WRAPPER: [&str; 2] = ["--", "dbus-run-session"];
    const CONFIG_FLAG: &str = "--config-file=";
    const SHELL_WRAPPER: [&str; 3] = ["--", "sh", "-c"];
    // Elements that would make a service activatable on the cage's bus, as text.
    const SERVICE_DIR_ELEMENTS: [&str; 3] =
        ["<servicedir", "<standard_session_servicedirs", "<include"];
    const SETTINGS_VAR: &str = "GSETTINGS_BACKEND";
    const SETTINGS_BACKEND: &str = "memory";
    // GTK's settings file under each XDG_CONFIG_DIRS entry, and what it must pin.
    const CONFIG_DIRS_VAR: &str = "XDG_CONFIG_DIRS";
    const GTK_SETTINGS_FILE: &str = "gtk-4.0/settings.ini";
    const GTK_SETTINGS_SECTION: &str = "Settings";
    const GTK_DPI_KEY: &str = "gtk-xft-dpi";
    const WANT_GTK_SETTINGS: [(&str, &str); 5] = [
        ("gtk-font-name", "Adwaita Sans 11"),
        ("gtk-xft-antialias", "1"),
        ("gtk-xft-hinting", "1"),
        ("gtk-xft-hintstyle", "hintslight"),
        ("gtk-xft-rgba", "none"),
    ];
    // The user-level config directory must be the one under the command's own HOME.
    const HOME_VAR: &str = "HOME";
    const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
    const USER_CONFIG_RELATIVE: &str = ".config";

    type EnvMap = std::collections::BTreeMap<std::ffi::OsString, Option<std::ffi::OsString>>;

    fn env_map(cmd: &Command) -> EnvMap {
        cmd.get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(OsStr::to_owned)))
            .collect()
    }

    // How `name` reaches the child, in words a failure message can carry.
    fn reach(envs: &EnvMap, name: &str) -> String {
        match envs.get(OsStr::new(name)) {
            None => format!(
                "not mentioned, so the child INHERITS this process's value ({:?})",
                std::env::var_os(name)
            ),
            Some(None) => "removed".to_owned(),
            Some(Some(value)) => format!("SET to {value:?}"),
        }
    }

    let local = CageSlots::new();
    let cwd = make_temp_cwd();
    let rt_a = CageRuntime::fresh(local.acquire(LOCAL_LIMIT));
    let rt_b = CageRuntime::fresh(local.acquire(LOCAL_LIMIT));
    let dir_a: &Path = &rt_a;
    let dir_b: &Path = &rt_b;
    let envs_a = env_map(&cage_command("true", &cwd, &rt_a));
    let envs_b = env_map(&cage_command("true", &cwd, &rt_b));

    // --- Non-vacuity: the map is the one `cage_command` builds ------------------
    let unset: Vec<String> = ISOLATION_VARS
        .into_iter()
        .filter(|name| !matches!(envs_a.get(OsStr::new(name)), Some(Some(_))))
        .map(|name| format!("{name}: {}", reach(&envs_a, name)))
        .collect();
    assert!(
        unset.is_empty(),
        "{GUARD}: the command `cage_command` built does not SET {unset:?}. Those are its own \
         per-smoke isolation, set on every call, so either `get_envs` is not reading the command \
         `cage_command` returned or that helper lost its isolation. Either way every check below \
         would be judging the wrong environment; fix this first."
    );

    // --- Check 1: the cage gets its runtime's directory, not this process's -----
    let want_a = Some(dir_a.as_os_str().to_owned());
    assert_eq!(
        envs_a.get(OsStr::new(RUNTIME_VAR)),
        Some(&want_a),
        "{GUARD}: check 1: {RUNTIME_VAR} is {}, where it must be SET to the CageRuntime's own \
         directory {}. Inherited, the cage puts its Wayland socket, WebKit proxy sockets and \
         runtime files in the developer's runtime directory beside every other parallel cage, \
         and the only thing between the app and the developer's session bus at \
         `$XDG_RUNTIME_DIR/bus` is the `dbus-run-session` wrapper: drop that too and a launch \
         hands off to their running wolf-gnome, and the smoke drives the wrong process. Restore \
         `.env(\"{RUNTIME_VAR}\", …)` in `cage_command`.",
        reach(&envs_a, RUNTIME_VAR),
        dir_a.display()
    );
    assert_ne!(
        std::env::var_os(RUNTIME_VAR).as_deref(),
        Some(dir_a.as_os_str()),
        "{GUARD}: check 1: the CageRuntime's directory {} IS this test process's own \
         {RUNTIME_VAR}. A cage there shares the developer's Wayland socket directory and runtime \
         files, and loses the defence in depth against a hand-off that the private directory \
         gives should the `dbus-run-session` wrapper go: the developer's `bus` sits right there \
         for GIO's fallback. `CageRuntime::fresh` must create a new directory under `temp_dir()`, never \
         reuse the inherited one.",
        dir_a.display()
    );

    // --- Check 2: one directory per cage ---------------------------------------
    assert_ne!(
        dir_a,
        dir_b,
        "{GUARD}: check 2: two CageRuntimes built back to back share the directory {}. Two cages \
         running at once would then share one Wayland socket directory (the second `cage` fails \
         on the first's lockfile) plus every capture, readback and ready file under it. \
         `CageRuntime::fresh` must name each directory uniquely, with the pid and a process-wide \
         sequence, and create it non-recursively so a collision fails at construction.",
        dir_a.display()
    );
    let want_b = Some(dir_b.as_os_str().to_owned());
    assert_eq!(
        envs_b.get(OsStr::new(RUNTIME_VAR)),
        Some(&want_b),
        "{GUARD}: check 2: the command built over the SECOND CageRuntime has {RUNTIME_VAR} {}, \
         not that runtime's own directory {}. `cage_command` must read the directory from the \
         runtime it is handed, never from a cached or shared value, or every parallel cage lands \
         in one directory again.",
        reach(&envs_b, RUNTIME_VAR),
        dir_b.display()
    );

    // --- Check 3: a real 0700 directory owned by this user ---------------------
    let cwd_meta = std::fs::metadata(&cwd).unwrap_or_else(|e| {
        panic!(
            "{GUARD}: could not stat the temp cwd {} that `make_temp_cwd` just created: {e}",
            cwd.display()
        )
    });
    let cwd_uid = std::os::unix::fs::MetadataExt::uid(&cwd_meta);
    for dir in [dir_a, dir_b] {
        let meta = std::fs::symlink_metadata(dir).unwrap_or_else(|e| {
            panic!(
                "{GUARD}: check 3: could not stat the runtime directory {}: {e}. It must exist from \
                 `CageRuntime::fresh` until the runtime is dropped.",
                dir.display()
            )
        });
        let mode = std::os::unix::fs::MetadataExt::mode(&meta) & 0o777;
        let uid = std::os::unix::fs::MetadataExt::uid(&meta);
        let kind = if meta.is_dir() {
            "a directory"
        } else if meta.file_type().is_symlink() {
            "a SYMLINK"
        } else {
            "NOT a directory"
        };
        assert!(
            meta.is_dir() && mode == 0o700 && uid == cwd_uid,
            "{GUARD}: check 3: the runtime directory {} is {kind} with mode {mode:o}, owned by uid \
             {uid}. It must be a real directory, mode 700, owned by uid {cwd_uid} (the owner of \
             the temp cwd). A symlink puts the cage's socket and the GIO bus fallback wherever it \
             points; a wider mode lets another user connect to the cage's Wayland socket and read \
             its captures, against the XDG base-directory spec's 0700. Create it with \
             `DirBuilder::new().mode(0o700)`.",
            dir.display()
        );
    }

    // --- Check 4: no `bus` for GIO to fall back to -----------------------------
    for dir in [dir_a, dir_b] {
        let fallback = dir.join(GIO_BUS_FALLBACK);
        match std::fs::symlink_metadata(&fallback) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            found => panic!(
                "{GUARD}: check 4: the runtime directory {} has an entry at {} ({found:?}), where it \
                 must have none. Inside the cage `dbus-run-session` sets {BUS_VAR} to its own \
                 private bus, so this entry is not consulted while that wrapper is in place. The \
                 check is the defence in depth for when it is not: with no address set, GIO \
                 connects to `$XDG_RUNTIME_DIR/bus`, so this entry would decide which session bus \
                 the app joins, and a socket or link reaching the developer's bus would hand the \
                 launch off to their running wolf-gnome, leaving the smoke driving the wrong \
                 process.",
                dir.display(),
                fallback.display()
            ),
        }
    }

    // --- Check 5: short enough for every socket under it -----------------------
    for dir in [dir_a, dir_b] {
        let len = dir.as_os_str().len();
        assert!(
            len <= RUNTIME_DIR_MAX_BYTES,
            "{GUARD}: check 5: the runtime directory {} is {len} bytes, over RUNTIME_DIR_MAX_BYTES \
             ({RUNTIME_DIR_MAX_BYTES}). Every socket made under it, the cage's Wayland socket first, \
             must fit a 108-byte `sun_path`, and that limit was measured from the longest one under \
             a live cage. Past it, binding fails and the cage or the app dies at startup. A long \
             TMPDIR is the usual cause; shorten it rather than raising the limit.",
            dir.display()
        );
    }

    // --- Check 6: no session-bus address ---------------------------------------
    assert_eq!(
        envs_a.get(OsStr::new(BUS_VAR)),
        Some(&None),
        "{GUARD}: check 6: {BUS_VAR} is {}, where it must be REMOVED. Inherited, the app joins \
         the developer's session bus and hands off to their running wolf-gnome; a DANGLING \
         inherited address kills the app before its first frame under WebKit's sandbox (see \
         `cage_command`). Restore its `.env_remove`.",
        reach(&envs_a, BUS_VAR)
    );

    // --- Check 7: no host display ----------------------------------------------
    let leaked: Vec<String> = DISPLAY_VARS
        .into_iter()
        .filter(|name| envs_a.get(OsStr::new(name)) != Some(&None))
        .map(|name| format!("{name}: {}", reach(&envs_a, name)))
        .collect();
    assert!(
        leaked.is_empty(),
        "{GUARD}: check 7: {leaked:?}. Each must be REMOVED from the cage's environment, because \
         each names the developer's own display to a process that must only ever see the cage's: \
         libwayland prefers WAYLAND_SOCKET over the WAYLAND_DISPLAY cage sets for its child, \
         wlroots picks a nested backend from WAYLAND_DISPLAY wherever WLR_BACKENDS is not pinned, \
         and with DISPLAY set GIO can autolaunch or find a session bus over X11. Restore the \
         missing `.env_remove`s in `cage_command`."
    );

    // --- Check 8: the ready file belongs to its own cage -----------------------
    for (envs, dir) in [(&envs_a, dir_a), (&envs_b, dir_b)] {
        let ready = match envs.get(OsStr::new(READY_VAR)) {
            Some(Some(value)) => Some(Path::new(value)),
            _ => None,
        };
        assert!(
            ready.is_some_and(|path| path.starts_with(dir) && path != dir),
            "{GUARD}: check 8: {READY_VAR} is {}, which is not a file inside this cage's runtime \
             directory {}. Outside it nothing ties the file to this cage: a path another cage can \
             also name lets that cage's app satisfy this cage's `wait_ready`, so input starts \
             before this app accepts it, and the file outlives the runtime directory's cleanup.",
            reach(envs, READY_VAR),
            dir.display()
        );
    }

    // --- Check 9: the packaged smokes keep all of it ---------------------------
    let mut undone = Vec::new();
    for arm in [GtkRuntimeEnv::Scrubbed, GtkRuntimeEnv::Inherited] {
        let envs = env_map(&packaged_cage_command("true", &cwd, &rt_a, arm));
        if envs.get(OsStr::new(RUNTIME_VAR)) != Some(&want_a) {
            undone.push(format!(
                "{arm:?}: {RUNTIME_VAR} is {}",
                reach(&envs, RUNTIME_VAR)
            ));
        }
        for name in std::iter::once(BUS_VAR).chain(DISPLAY_VARS) {
            if envs.get(OsStr::new(name)) != Some(&None) {
                undone.push(format!("{arm:?}: {name} is {}", reach(&envs, name)));
            }
        }
    }
    assert!(
        undone.is_empty(),
        "{GUARD}: check 9: `packaged_cage_command` undoes the cage isolation: {undone:#?}\n\
         Every packaged smoke runs the STORE binary, which is the binary a developer also has \
         installed and running, so a packaged cage that reaches the developer's runtime directory \
         or display hands its launch off to that running copy and passes against the wrong \
         process. Either a name in PACKAGED_SCRUB_ALWAYS, PACKAGED_SCRUB_GTK_RUNTIME or \
         PACKAGED_SCRUB_FONT_STACK is one of these isolation variables (take it off: the scrub \
         lists hold devShell exports, never harness isolation), or `packaged_cage_command` no \
         longer builds on `cage_command` with the runtime it was handed."
    );

    // --- Checks 10 to 16 read four commands, each with the directory it was built over
    let mut built: Vec<(String, &Path, Vec<std::ffi::OsString>, EnvMap)> = Vec::new();
    for (builder, dir, cmd) in [
        (
            "cage_command over runtime A".to_owned(),
            dir_a,
            cage_command("true", &cwd, &rt_a),
        ),
        (
            "cage_command over runtime B".to_owned(),
            dir_b,
            cage_command("true", &cwd, &rt_b),
        ),
        (
            "packaged_cage_command(Scrubbed) over runtime A".to_owned(),
            dir_a,
            packaged_cage_command("true", &cwd, &rt_a, GtkRuntimeEnv::Scrubbed),
        ),
        (
            "packaged_cage_command(Inherited) over runtime A".to_owned(),
            dir_a,
            packaged_cage_command("true", &cwd, &rt_a, GtkRuntimeEnv::Inherited),
        ),
    ] {
        let args = cmd.get_args().map(OsStr::to_owned).collect();
        built.push((builder, dir, args, env_map(&cmd)));
    }

    // --- Check 10: every scenario runs under its cage's own session bus --------
    let config_at = BUS_WRAPPER.len();
    let want_len = BUS_WRAPPER.len() + 1 + SHELL_WRAPPER.len() + 1;
    let config_of = |args: &[std::ffi::OsString]| -> Option<PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        if args.len() != want_len
            || args[..config_at]
                .iter()
                .map(|arg| arg.as_os_str())
                .ne(BUS_WRAPPER.into_iter().map(OsStr::new))
            || args[config_at + 1..want_len - 1]
                .iter()
                .map(|arg| arg.as_os_str())
                .ne(SHELL_WRAPPER.into_iter().map(OsStr::new))
        {
            return None;
        }
        let rest = args[config_at]
            .as_bytes()
            .strip_prefix(CONFIG_FLAG.as_bytes())?;
        Some(PathBuf::from(OsStr::from_bytes(rest)))
    };
    let unwrapped: Vec<String> = built
        .iter()
        .filter(|(_, _, args, _)| config_of(args).is_none())
        // The last argument is the whole readiness prelude plus the scenario, so only
        // the leading ones and the count are printed.
        .map(|(builder, _, args, _)| {
            let head: Vec<_> = args.iter().take(want_len - 1).collect();
            format!("{builder}: {} args, starting {head:?}", args.len())
        })
        .collect();
    assert!(
        unwrapped.is_empty(),
        "{GUARD}: check 10: these commands do not pass `cage` exactly {BUS_WRAPPER:?}, one \
         `{CONFIG_FLAG}<path>`, {SHELL_WRAPPER:?} and then the one script argument: \
         {unwrapped:#?}\n\
         Three distinguishable losses. Without `dbus-run-session` around `sh`, each cage's \
         scenario has no session bus of its own. That wrapper is what blocks a launch from \
         handing off to the developer's running wolf-gnome; without it GIO finds a bus some other \
         way, autolaunching one through the cage's own Xwayland `DISPLAY` (a stray `dbus-daemon` \
         and an Xwayland start per launch). Without the `{CONFIG_FLAG}` argument the bus daemon \
         reads whatever configuration the host's /etc/dbus-1 holds: the smokes pass on a NixOS \
         host, activating whatever its service directories offer, and every one exits 127 in a \
         container built from the flake, which has no /etc/dbus-1. Any other argument count \
         means the script is no longer the single `sh -c` argument, so `sh` would run something \
         other than the readiness prelude plus the scenario. Restore `--`, `dbus-run-session`, \
         `{CONFIG_FLAG}` plus `smoke_bus_config(runtime)`, `--` ahead of `sh -c` in \
         `cage_command`."
    );

    // --- Check 11: the configuration is the cage's own file -------------------
    let mut configs: Vec<(&str, &Path, PathBuf)> = Vec::new();
    for (builder, dir, args, _) in &built {
        let conf = config_of(args).expect("check 10 established that every command names one");
        let kind = match std::fs::symlink_metadata(&conf) {
            Ok(meta) if meta.is_file() => None,
            Ok(meta) if meta.file_type().is_symlink() => Some("a SYMLINK".to_owned()),
            Ok(_) => Some("NOT a regular file".to_owned()),
            Err(e) => Some(format!("MISSING ({e})")),
        };
        let place = if !conf.is_absolute() {
            Some("RELATIVE, so it resolves against each smoke's cwd")
        } else if conf.parent() == Some(*dir) {
            None
        } else if conf.parent() == Some(dir_a) || conf.parent() == Some(dir_b) {
            Some("in the OTHER runtime's directory, so one cage's file serves another")
        } else {
            Some("OUTSIDE its runtime directory")
        };
        assert!(
            kind.is_none() && place.is_none(),
            "{GUARD}: check 11: {builder} hands `dbus-run-session` {}, which is {}{}. It must be \
             an absolute path to a regular file directly inside {}, the runtime directory the \
             command was built over, as `smoke_bus_config` writes it. A file installed beside \
             `dbus-run-session` (its share/dbus-1/session.conf) or the host's /etc/dbus-1 carries \
             service directories, through which a NixOS host's portal supplies the dpi whose \
             absence blanks a preview created after launch; a file outside the runtime \
             directory outlives the cage \
             and is shared between cages; a missing one stops the bus daemon before the scenario \
             starts.",
            conf.display(),
            kind.as_deref().unwrap_or("a regular file"),
            place.map(|p| format!(", {p}")).unwrap_or_default(),
            dir.display()
        );
        configs.push((builder.as_str(), *dir, conf));
    }

    // --- Check 12: nothing on the cage's bus is activatable ---------------------
    for (builder, dir, conf) in &configs {
        let text = std::fs::read_to_string(conf).unwrap_or_else(|e| {
            panic!(
                "{GUARD}: check 12: could not read {builder}'s bus configuration {}: {e}. Check 11 \
                 just found it a regular file.",
                conf.display()
            )
        });
        let want_listen = format!("<listen>unix:dir={}</listen>", dir.display());
        let named: Vec<&str> = SERVICE_DIR_ELEMENTS
            .into_iter()
            .filter(|element| text.contains(element))
            .collect();
        assert!(
            text.contains(&want_listen) && named.is_empty(),
            "{GUARD}: check 12: {builder}'s bus configuration {} {}, and names {named:?} of \
             {SERVICE_DIR_ELEMENTS:?}. It must carry `{want_listen}` and none of them. Each named \
             element makes services activatable on the cage's bus: `<standard_session_servicedirs` \
             expands the inherited XDG_DATA_DIRS, which on a NixOS host activates the HOST's \
             xdg-desktop-portal, and that portal supplies the `gtk-xft-dpi` the app's own pin \
             exists for, so `preview_created_after_launch_renders_without_a_portal_under_cage` \
             passes there with the pin deleted and fails in a container; an `<include` can \
             bring either element back. A missing listen element \
             means this is not the file `smoke_bus_config` writes from SMOKE_BUS_CONFIG_TEMPLATE, \
             or that the socket is made outside the cage's own directory. File contents:\n{text}",
            conf.display(),
            if text.contains(&want_listen) {
                "listens in its own runtime directory"
            } else {
                "does NOT listen in its own runtime directory"
            }
        );
    }

    // --- Check 13: GSettings is the in-memory backend, on every builder -------
    let want_backend = Some(std::ffi::OsString::from(SETTINGS_BACKEND));
    let wrong_backend: Vec<String> = built
        .iter()
        .filter(|(_, _, _, envs)| envs.get(OsStr::new(SETTINGS_VAR)) != Some(&want_backend))
        .map(|(builder, _, _, envs)| format!("{builder}: {}", reach(envs, SETTINGS_VAR)))
        .collect();
    assert!(
        wrong_backend.is_empty(),
        "{GUARD}: check 13: {SETTINGS_VAR} must be SET to {SETTINGS_BACKEND:?} on every command, \
         and is not on {wrong_backend:#?}\n\
         Inherited or removed, GIO picks dconf where its module is installed, which reads the \
         user database under XDG_CONFIG_HOME (the smoke home's empty `.config`, check 16) and \
         writes through a `ca.desrt.dconf` service the cage's bus cannot activate, and falls \
         back to memory with a log line where it is not — so the backend becomes a property of \
         the host. It does not decide what GTK renders with: measured on a cage's bus, GTK takes \
         no font or xft value from GSettings, which is why the GTK settings file of checks 14 \
         and 15 exists. Restore \
         `.env(\"{SETTINGS_VAR}\", SMOKE_GSETTINGS_BACKEND)` in `cage_command`, and keep the name \
         off the packaged scrub lists: it is harness isolation, not a devShell export."
    );

    // --- Check 14: GTK's system settings come from the cage's own directory ----
    let mut settings_files: Vec<(&str, PathBuf)> = Vec::new();
    for (builder, dir, _, envs) in &built {
        let config_dirs = match envs.get(OsStr::new(CONFIG_DIRS_VAR)) {
            Some(Some(value)) => Some(PathBuf::from(value)),
            _ => None,
        };
        let file = config_dirs.as_ref().map(|d| d.join(GTK_SETTINGS_FILE));
        let is_file = file
            .as_ref()
            .is_some_and(|f| std::fs::symlink_metadata(f).is_ok_and(|meta| meta.is_file()));
        let in_runtime = config_dirs
            .as_ref()
            .is_some_and(|d| d.is_absolute() && d.parent() == Some(*dir));
        assert!(
            in_runtime && is_file,
            "{GUARD}: check 14: {builder} has {CONFIG_DIRS_VAR} {}, and {GTK_SETTINGS_FILE} under \
             it is {}. It must be SET to ONE absolute directory directly inside {}, the runtime \
             directory the command was built over, holding a regular file at {GTK_SETTINGS_FILE}, \
             as `smoke_gtk_config_dirs` writes it. Inherited, GTK reads the host's system \
             settings (a NixOS /etc/xdg can carry one) and leaves the font and xft settings unset \
             where it has none, which put 27 pixels of glyph residue into the picker smoke's \
             reference band; outside the runtime directory the file outlives the cage and is \
             shared between cages. Restore `.env(\"{CONFIG_DIRS_VAR}\", \
             smoke_gtk_config_dirs(runtime))` in `cage_command`.",
            reach(envs, CONFIG_DIRS_VAR),
            if is_file {
                "a regular file"
            } else {
                "MISSING or not a regular file"
            },
            dir.display()
        );
        settings_files.push((
            builder.as_str(),
            file.expect("check 14 established the file"),
        ));
    }

    // --- Check 15: those settings pin the font and xft keys, and never the dpi --
    for (builder, file) in &settings_files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| {
            panic!(
                "{GUARD}: check 15: could not read {builder}'s GTK settings {}: {e}. Check 14 just \
                 found it a regular file.",
                file.display()
            )
        });
        let mut section = None;
        let mut got = std::collections::BTreeMap::new();
        let mut outside = Vec::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = Some(name);
            } else if let Some((key, value)) = line.split_once('=') {
                if section == Some(GTK_SETTINGS_SECTION) {
                    got.insert(key.trim(), value.trim());
                } else {
                    outside.push(line);
                }
            } else {
                outside.push(line);
            }
        }
        let want: std::collections::BTreeMap<&str, &str> = WANT_GTK_SETTINGS.into_iter().collect();
        let verdict = if got.contains_key(GTK_DPI_KEY) {
            "it SETS gtk-xft-dpi, which satisfies the app's own pin and lets every preview smoke \
             pass with that pin deleted from crates/wolf-gnome/src/app.rs"
        } else if !outside.is_empty() {
            "some lines sit outside the [Settings] section, where GTK ignores them"
        } else {
            "its keys or values differ from the pinned set"
        };
        assert!(
            got == want && outside.is_empty(),
            "{GUARD}: check 15: {builder}'s GTK settings {} hold {got:?} under \
             [{GTK_SETTINGS_SECTION}] and {outside:?} outside it: {verdict}. They must be exactly \
             {want:?}, the font and xft settings a portal would otherwise supply, and must NOT \
             set {GTK_DPI_KEY}: the preview smokes are the end-to-end evidence for the app's \
             dpi pin, and only an unset dpi reaches it. File contents:\n{text}",
            file.display()
        );
    }

    // --- Check 16: no user-level GTK settings can override them ----------------
    let wrong_home: Vec<String> = built
        .iter()
        .filter(|(_, _, _, envs)| {
            let home = match envs.get(OsStr::new(HOME_VAR)) {
                Some(Some(value)) => Some(Path::new(value).join(USER_CONFIG_RELATIVE)),
                _ => None,
            };
            let config_home = match envs.get(OsStr::new(CONFIG_HOME_VAR)) {
                Some(Some(value)) => Some(PathBuf::from(value)),
                _ => None,
            };
            home.is_none() || home != config_home
        })
        .map(|(builder, _, _, envs)| {
            format!(
                "{builder}: {CONFIG_HOME_VAR} is {}, {HOME_VAR} is {}",
                reach(envs, CONFIG_HOME_VAR),
                reach(envs, HOME_VAR)
            )
        })
        .collect();
    assert!(
        wrong_home.is_empty(),
        "{GUARD}: check 16: {CONFIG_HOME_VAR} must be SET to the `{USER_CONFIG_RELATIVE}` \
         directory under the {HOME_VAR} the same command sets, and is not on {wrong_home:#?}\n\
         GTK lets a user-level gtk-4.0/settings.ini override the system ones, so an inherited \
         {CONFIG_HOME_VAR} hands a smoke the developer's own GTK settings — a gtk-xft-dpi among \
         them masks the app's pin — and moves where a wolf-config.yaml leak would land away from \
         the directory `single_pane_file_survives_relaunch_under_cage` checks. Restore \
         `.env(\"{CONFIG_HOME_VAR}\", smoke_home(cwd).join(\".config\"))` in `cage_command`."
    );

    if let Some(parent) = cwd.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

/// [`parse_smoke_jobs`] honours `WOLF_SMOKE_JOBS`, and [`CageSlots`] never admits
/// more cages than its limit.
///
/// ## What it closes
///
/// The cage slots are the only thing between the smokes and an oversubscribed host.
/// A semaphore that admits one cage too many passes on an idle machine and stretches
/// every fixed settle on a loaded one, so its failures arrive as scattered flakes in
/// unrelated smokes rather than as a failure naming it. A parse that drops the
/// override, or accepts `0`, either ignores the knob the docs give for serial runs or
/// blocks every smoke forever.
///
/// ## What it checks
///
/// **(a) The parse**, as a table. `(Some("1"), 32)` is `Ok(1)`. `(Some(" 12 "), 32)`
/// is `Ok(12)`, above the cap; falsify with `.min(DEFAULT_SMOKE_JOBS_CAP)` on the
/// override. `(None, 32)` is the value computed from
/// [`DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE`] and [`DEFAULT_SMOKE_JOBS_CAP`], which is
/// also asserted to be the literal `16`, the default measured on that 32-thread host;
/// falsify by re-tuning either constant without the measurement. `(None, 12)` is
/// computed the same way and catches a default that ignores `parallelism` or divides
/// by another number; falsify with `None => Ok(DEFAULT_SMOKE_JOBS_CAP)`. `(None, 1)`
/// and `(None, 3)` are `1`; falsify by removing the clamp's lower bound. `"0"`, `""`,
/// `"x"` and `"-1"` are each an `Err`; falsify by accepting `0`.
///
/// **(b) The bound**, at limits 1 and 3. Six threads contend for one local
/// [`CageSlots`]. Each holder waits, under a deadline, until every thread has arrived
/// and the run is full (or everyone has been admitted), then keeps its slot a moment
/// longer so that a semaphore admitting one more has the time to do it. The most
/// holders ever inside at once must equal the limit exactly. Falsify: in the wait
/// predicate of `acquire`, change `running >= limit` to `running > limit`. A
/// semaphore that admits fewer than its limit, or never wakes a waiter, fails on a
/// holder's deadline or on the overall one instead.
///
/// ## Timing
///
/// Every blocking wait here ends at a deadline, and every thread that could block in
/// a broken `acquire` is abandoned rather than joined, so a broken semaphore fails
/// loudly instead of hanging the suite; that is why the slots are leaked to
/// `'static`. The extra hold only widens the window for an over-admission: a correct
/// semaphore can never exceed its limit, however long the hold.
///
/// ## What it needs
///
/// No compositor, no [`require_smoke_env`], no global cage slot, no subprocess and no
/// environment: it builds its own [`CageSlots`] and uses only std threads.
#[test]
fn cage_slots_bound_concurrency() {
    use std::sync::{mpsc, Arc};
    use std::time::Instant;

    const GUARD: &str = "cage_slots_bound_concurrency";
    // The host the default was measured on, and the default measured there.
    const MEASUREMENT_HOST_THREADS: usize = 32;
    const EXPECT_DEFAULT_JOBS_ON_MEASUREMENT_HOST: usize = 16;
    // A host between the clamp's two bounds, where the division itself decides.
    const MID_HOST_THREADS: usize = 12;
    // Contenders per bound run: more than either limit, so some always wait.
    const CONTENDERS: usize = 6;
    // Every blocking wait in this guard ends here.
    const DEADLINE: Duration = Duration::from_secs(5);
    // How long a holder keeps its slot once the run is full.
    const OVERADMIT_WINDOW: Duration = Duration::from_millis(100);

    // --- (a) The WOLF_SMOKE_JOBS parse ------------------------------------------
    let default_on_host = (MEASUREMENT_HOST_THREADS / DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE)
        .clamp(1, DEFAULT_SMOKE_JOBS_CAP);
    assert_eq!(
        default_on_host, EXPECT_DEFAULT_JOBS_ON_MEASUREMENT_HOST,
        "{GUARD}: (a) DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE ({DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE}) and \
         DEFAULT_SMOKE_JOBS_CAP ({DEFAULT_SMOKE_JOBS_CAP}) give {default_on_host} cage slots on \
         the {MEASUREMENT_HOST_THREADS}-thread host the default was measured on, but the measured \
         default there is {EXPECT_DEFAULT_JOBS_ON_MEASUREMENT_HOST}. A constant moved without a \
         measurement. If a new measurement moved the default, update this literal together with \
         the table in those constants' doc comment."
    );
    let ok_rows: [(Option<&str>, usize, usize, &str); 6] = [
        (Some("1"), MEASUREMENT_HOST_THREADS, 1, "an explicit 1 is the serial run"),
        (
            Some(" 12 "),
            MEASUREMENT_HOST_THREADS,
            12,
            "an explicit value is trimmed and is NOT capped at DEFAULT_SMOKE_JOBS_CAP: the cap \
             shapes the default only, and capping the override silently refuses a stress run",
        ),
        (
            None,
            MEASUREMENT_HOST_THREADS,
            default_on_host,
            "unset, the default is one cage per DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE threads, capped",
        ),
        (
            None,
            MID_HOST_THREADS,
            (MID_HOST_THREADS / DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE).clamp(1, DEFAULT_SMOKE_JOBS_CAP),
            "the default scales with the host, so one that ignores `parallelism`, or divides it by \
             another number, answers something else here",
        ),
        (
            None,
            1,
            1,
            "a one-thread host still runs one cage: without the clamp's lower bound the default \
             is 0 and every smoke blocks in `acquire` forever",
        ),
        (
            None,
            3,
            (3 / DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE).clamp(1, DEFAULT_SMOKE_JOBS_CAP),
            "a host with fewer threads than one cage's share still runs one cage, for the same \
             reason",
        ),
    ];
    for (raw, parallelism, expected, why) in ok_rows {
        let got = parse_smoke_jobs(raw, parallelism);
        assert_eq!(
            got,
            Ok(expected),
            "{GUARD}: (a) parse_smoke_jobs({raw:?}, {parallelism}) answered {got:?}, expected \
             Ok({expected}): {why}."
        );
    }
    let err_rows: [(&str, &str); 4] = [
        (
            "0",
            "zero slots admit no cage at all, so every smoke would block in `acquire` with no \
             failure ever reported",
        ),
        (
            "",
            "a set-but-empty value is a mistake on the caller's command line, and reading it as \
             unset would run the default the caller was trying to override",
        ),
        ("x", "a typo would silently become the default"),
        ("-1", "a negative count is not a count"),
    ];
    for (raw, why) in err_rows {
        let got = parse_smoke_jobs(Some(raw), MEASUREMENT_HOST_THREADS);
        assert!(
            got.is_err(),
            "{GUARD}: (a) {SMOKE_JOBS_ENV}={raw:?} parsed as {got:?}, where it must be an Err that \
             `smoke_jobs` turns into a panic naming the variable: {why}."
        );
    }

    // --- (b) Never more than `limit` holders at once ----------------------------
    for limit in [1, 3] {
        let slots: &'static CageSlots = Box::leak(Box::new(CageSlots::new()));
        let arrived = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let inside = Arc::new(AtomicUsize::new(0));
        let most_inside = Arc::new(AtomicUsize::new(0));
        let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
        for _ in 0..CONTENDERS {
            let arrived = Arc::clone(&arrived);
            let entered = Arc::clone(&entered);
            let inside = Arc::clone(&inside);
            let most_inside = Arc::clone(&most_inside);
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                arrived.fetch_add(1, Ordering::SeqCst);
                let slot = slots.acquire(limit);
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                most_inside.fetch_max(now, Ordering::SeqCst);
                entered.fetch_add(1, Ordering::SeqCst);
                let give_up = Instant::now() + DEADLINE;
                let outcome = loop {
                    let all_arrived = arrived.load(Ordering::SeqCst) == CONTENDERS;
                    let full = inside.load(Ordering::SeqCst) >= limit;
                    let all_admitted = entered.load(Ordering::SeqCst) == CONTENDERS;
                    if all_arrived && (full || all_admitted) {
                        break Ok(());
                    }
                    if Instant::now() >= give_up {
                        break Err(format!(
                            "a holder waited {DEADLINE:?} for the run to fill, with {} inside \
                             against a limit of {limit}, {} of {CONTENDERS} admitted so far and {} \
                             arrived",
                            inside.load(Ordering::SeqCst),
                            entered.load(Ordering::SeqCst),
                            arrived.load(Ordering::SeqCst)
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                // Keep the slot a moment longer, so a semaphore that admits one more
                // than `limit` has the time to do it while this holder is still in.
                std::thread::sleep(OVERADMIT_WINDOW);
                inside.fetch_sub(1, Ordering::SeqCst);
                drop(slot);
                let _ = done_tx.send(outcome);
            });
        }
        drop(done_tx);

        let give_up = Instant::now() + DEADLINE * 2;
        for finished in 0..CONTENDERS {
            match done_rx.recv_timeout(give_up.saturating_duration_since(Instant::now())) {
                Ok(Ok(())) => {}
                Ok(Err(why)) => panic!(
                    "{GUARD}: (b) at limit {limit}, {why}. The semaphore admits FEWER holders than \
                     its limit, or does not wake a waiter when a slot frees (a release without \
                     `notify_all`). Under the smokes that runs the suite at a fraction of \
                     WOLF_SMOKE_JOBS, or leaves a smoke that never starts."
                ),
                Err(e) => panic!(
                    "{GUARD}: (b) at limit {limit}, only {finished} of {CONTENDERS} contenders \
                     finished within {:?} ({e}); {} were admitted in all and {} are inside now. \
                     The rest are blocked in `acquire` while slots are free: a release that never \
                     notifies, or a wait predicate that never clears. Under the smokes that hangs \
                     the suite with no failure reported.",
                    DEADLINE * 2,
                    entered.load(Ordering::SeqCst),
                    inside.load(Ordering::SeqCst)
                ),
            }
        }
        let most = most_inside.load(Ordering::SeqCst);
        assert_eq!(
            most, limit,
            "{GUARD}: (b) at limit {limit}, {most} holders were inside at once. Above the limit, \
             the semaphore admits one cage too many (`running > limit` where `running >= limit` \
             belongs): every extra cage stretches the others' fixed settles, so the smokes pass on \
             an idle host and flake on a loaded one, and WOLF_SMOKE_JOBS=1 no longer runs them \
             serially. Below it, a holder's deadline would have fired first, so this reading is \
             not reachable."
        );
    }
}

/// The head of a launch line: a double quote and a `$`, as in `"$WOLF_BIN" &`.
///
/// This and the six needles below are assembled from fragments for the reason
/// [`CAGE_SPAWN_NEEDLE`] is: [`every_launch_waits_for_readiness_before_its_first_input`]
/// scans this binary's own sources, so a needle spelled contiguously could match the
/// line that declares it.
const LAUNCH_HEAD: &str = concat!("\"", "$");

/// The start of the readiness call that must follow every launch.
const READY_CALL_NEEDLE: &str = concat!("wait_ready", " \"$app_pid\"");

/// The start of `wait_ready`'s definition. Matched only at the start of a trimmed
/// line, because the pointer helper's `hwait_ready() {` contains this text.
const READY_DEF_NEEDLE: &str = concat!("wait_", "ready() {");

/// The loop line of the bare `if wtype "" …` launch probe, matched as a whole
/// trimmed line. `wtype ""` itself legitimately appears inside the prelude's priming
/// step, so the bare command is not the needle.
const OLD_PROBE_LINE: &str = concat!("if wtype ", "\"\"", " >/dev/null 2>&1; then");

/// The line that closes every scenario's raw string.
const RAW_STRING_END: &str = concat!("\"", "#;");

/// The end of the line that opens a scenario's raw string, used only to name the
/// scenario in a failure message.
const RAW_STRING_OPEN: &str = concat!("r", "#\"");

/// The first command of each kind of input or capture a scenario sends. A launch's
/// readiness wait must come before the first of these.
const FIRST_INPUT_HEADS: [&str; 4] = ["wtype", "hsend", "hstep", "grim"];

/// The number of launch lines this binary's sources hold, counted against them: every
/// `"$WOLF_BIN" &` in a scenario plus the packaged plugins smoke's `"$app_bin" &`.
/// A floor rather than an exact count, so a new smoke does not have to edit it;
/// deleting a smoke does.
const EXPECT_MIN_LAUNCH_SITES: usize = 62;

/// The line that captures a launch's pid, matched as a whole trimmed line. Every
/// launch line is followed by one, and every one is followed by the readiness call.
const PID_CAPTURE_LINE: &str = concat!("app_pid", "=$!");

/// Whether `line` launches the app: trimmed and with a trailing ` &` removed, it is
/// a single quoted expansion such as `"$WOLF_BIN"`.
fn is_launch_line(line: &str) -> bool {
    let trimmed = line.trim();
    let command = trimmed.strip_suffix(" &").unwrap_or(trimmed);
    command.len() > LAUNCH_HEAD.len()
        && command.starts_with(LAUNCH_HEAD)
        && command.ends_with('"')
        && !command.contains(' ')
}

/// Whether `line` is a shell command rather than a blank line or a `#` comment.
fn is_shell_command(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

/// The scenario const enclosing the 0-based line `index`, for failure messages:
/// the name of the nearest preceding `const NAME: &str = r#"` line, and its 1-based
/// line number.
fn enclosing_scenario(lines: &[&str], index: usize) -> String {
    lines[..index]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.trim_end().ends_with(RAW_STRING_OPEN))
        .map_or_else(
            || "no enclosing raw string found".to_owned(),
            |(i, line)| {
                let trimmed = line.trim();
                let name = trimmed
                    .strip_prefix("const ")
                    .and_then(|rest| rest.split(':').next())
                    .unwrap_or(trimmed);
                format!("{name}, opened at line {}", i + 1)
            },
        )
}

/// Enforces mechanically that every launch in every scenario waits for the app's
/// readiness file before its first input, with no fixed sleep standing in for it.
///
/// ## What it checks
///
/// - **Every launch line is followed by `wait_ready "$app_pid"`** before the first
///   line that starts with `wtype`, `hsend`, `hstep` or `grim`, the next launch, or
///   the end of the scenario's raw string. A launch line is one whose trimmed form,
///   minus a trailing ` &`, is a single quoted expansion (`"$WOLF_BIN"`,
///   `"$app_bin"`).
/// - **Nothing sleeps between the launch and that `wait_ready`.** A sleep there is a
///   fixed launch budget standing in for the readiness file, and on a loaded host it
///   is either too short or wasted.
/// - **There are at least [`EXPECT_MIN_LAUNCH_SITES`] launch lines**, so a matcher
///   that stops recognizing launches cannot pass by checking none.
/// - **`wait_ready` is defined exactly once, inside [`READINESS_PRELUDE`]**, so no
///   scenario shadows it with a copy that does not read `WOLF_READY_FILE`.
/// - **The bare `if wtype "" …` probe loop appears nowhere.** It returns as soon as
///   cage accepts a virtual keyboard, before the window maps, and leaves the scenario
///   to type after a fixed settle: the budget this guard exists to keep out.
/// - **The first command after every `app_pid=$!` is `wait_ready "$app_pid"`**, with
///   only blank and `#` comment lines between. The first check's scan stops only at a
///   line that itself starts with an input command, so a scenario helper that wraps
///   input (`fill_buffer`, `probe_winnr`, `wv_chord`, `quit_app`) run before the
///   wait would slip past it. Falsifying edit: insert a line `true` between any
///   `app_pid=$!` and its `wait_ready` — only this check fires.
/// - **There are exactly as many `app_pid=$!` lines as recognised launch lines.**
///   Every launch captures its pid, so a surplus capture is a launch the matcher
///   misses, such as `"$WOLF_BIN" 2>"$LOG" &`, which every other check here then
///   skips. The floor alone catches that only while no new smoke has raised the
///   count above it. This runs before the floor check, so its more specific message
///   is the one a reader sees. Falsifying edit: add a redirect to any launch, e.g.
///   `"$WOLF_BIN" 2>/dev/null &`.
///
/// ## What it deliberately does not cover
///
/// The two-window smoke's mid-run New Window is not a launch line, and the
/// readiness file is written once per process, so nothing here could hold it to a
/// readiness wait; its settles stay. A Neovim restart inside a running app is the
/// same case. The `#[ignore]`d packaged smokes launch through [`cage_command`] too,
/// so their launch lines are checked and counted like any other.
///
/// ## The premise
///
/// The guard reads text, so it trusts that each scenario's launches and inputs are
/// lines of its raw string in execution order. The pointer smokes are the one
/// place where that needs care: their launch and `wait_ready` sit in
/// [`POINTER_PREAMBLE`], whose raw string ends before any body's `hsend`, so the
/// scan stops at that raw string's end rather than reading into the body.
///
/// Pure: no compositor, no cage slot, no environment, no subprocess.
#[test]
fn every_launch_waits_for_readiness_before_its_first_input() {
    let sources = read_test_sources("smoke_headless");
    assert_reads_every_module(
        "every_launch_waits_for_readiness_before_its_first_input",
        &sources,
    );

    let mut launch_sites = Vec::new();
    let mut capture_count = 0;
    let mut unpaired_captures = Vec::new();
    let mut missing_wait = Vec::new();
    let mut sleep_before_wait = Vec::new();
    let mut capture_not_followed_by_wait = Vec::new();
    let mut definitions = Vec::new();
    let mut old_probes = Vec::new();
    for source in &sources {
        let file = source.path.as_str();
        let lines: Vec<&str> = source.text.lines().collect();

        let launches: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| is_launch_line(line))
            .map(|(i, _)| i)
            .collect();
        let pid_captures: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim() == PID_CAPTURE_LINE)
            .map(|(i, _)| i)
            .collect();
        launch_sites.extend(launches.iter().map(|i| format!("{file}:{}", i + 1)));
        capture_count += pid_captures.len();

        unpaired_captures.extend(
            pid_captures
                .iter()
                .filter(|&&capture| {
                    !lines[..capture]
                        .iter()
                        .rev()
                        .find(|line| is_shell_command(line))
                        .is_some_and(|line| is_launch_line(line))
                })
                .map(|&capture| {
                    let context = enclosing_scenario(&lines, capture);
                    format!("{file} line {} ({context})", capture + 1)
                }),
        );

        for &launch in &launches {
            let window_end = lines[launch + 1..]
                .iter()
                .position(|line| {
                    let trimmed = line.trim();
                    trimmed == RAW_STRING_END
                        || is_launch_line(line)
                        || FIRST_INPUT_HEADS
                            .iter()
                            .any(|head| trimmed.starts_with(head))
                })
                .map_or(lines.len(), |offset| launch + 1 + offset);
            let context = enclosing_scenario(&lines, launch);
            let stopped_at = lines
                .get(window_end)
                .map_or("end of file", |line| line.trim());

            let Some(ready) =
                (launch + 1..window_end).find(|&i| lines[i].trim().starts_with(READY_CALL_NEEDLE))
            else {
                missing_wait.push(format!(
                    "{file} line {} ({context}): `{}` reaches line {} (`{stopped_at}`) with no \
                     `{READY_CALL_NEEDLE}` in between",
                    launch + 1,
                    lines[launch].trim(),
                    window_end + 1
                ));
                continue;
            };
            for (i, line) in lines.iter().enumerate().take(ready).skip(launch + 1) {
                if line.trim().starts_with("sleep") {
                    sleep_before_wait.push(format!(
                        "{file} line {} ({context}): `{}` between the launch at line {} and its \
                         `{READY_CALL_NEEDLE}` at line {}",
                        i + 1,
                        line.trim(),
                        launch + 1,
                        ready + 1
                    ));
                }
            }
        }

        capture_not_followed_by_wait.extend(pid_captures.iter().filter_map(|&capture| {
            let next = lines[capture + 1..]
                .iter()
                .enumerate()
                .find(|(_, line)| is_shell_command(line))
                .map(|(offset, line)| (capture + 1 + offset, line.trim()));
            let context = enclosing_scenario(&lines, capture);
            match next {
                Some((_, line)) if line.starts_with(READY_CALL_NEEDLE) => None,
                Some((i, line)) => Some(format!(
                    "{file} line {} ({context}): `{PID_CAPTURE_LINE}` is followed at line {} \
                     by `{line}`",
                    capture + 1,
                    i + 1
                )),
                None => Some(format!(
                    "{file} line {} ({context}): `{PID_CAPTURE_LINE}` is followed by no \
                     command at all",
                    capture + 1
                )),
            }
        }));

        definitions.extend(
            lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.trim().starts_with(READY_DEF_NEEDLE))
                .map(|(i, _)| format!("{file}:{}", i + 1)),
        );

        old_probes.extend(
            lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.trim() == OLD_PROBE_LINE)
                .map(|(i, _)| format!("{file} line {} ({})", i + 1, enclosing_scenario(&lines, i))),
        );
    }

    assert_eq!(
        capture_count,
        launch_sites.len(),
        "found {} `{PID_CAPTURE_LINE}` lines but {} recognised launch lines. Every launch \
         captures its pid, so a surplus capture is a launch the matcher no longer recognises \
         (a `\"$WOLF_BIN\" &` reworded, e.g. `\"$WOLF_BIN\" 2>\"$LOG\" &`), and every other check \
         here skips it: that launch could type before readiness and stay green. A surplus launch \
         is one whose pid is never captured, so its `wait_ready` waits on another launch's pid \
         or on none. Captures whose preceding command is not a recognised launch: [{}]. Launch \
         lines found: {:?}",
        capture_count,
        launch_sites.len(),
        unpaired_captures.join(", "),
        launch_sites
    );

    assert!(
        launch_sites.len() >= EXPECT_MIN_LAUNCH_SITES,
        "found {} launch lines, expected at least {EXPECT_MIN_LAUNCH_SITES}. Either a smoke was \
         deleted (lower EXPECT_MIN_LAUNCH_SITES to the new count) or the launch matcher no \
         longer recognizes this binary's launches (a `\"$WOLF_BIN\" &` reworded, e.g. with a \
         redirect), in which case every check below is checking fewer launches than exist. \
         Launch lines found: {:?}",
        launch_sites.len(),
        launch_sites
    );

    assert!(
        missing_wait.is_empty(),
        "{} launch(es) send input, capture, relaunch or end the scenario without first calling \
         `{READY_CALL_NEEDLE}`. Such a scenario types into a window that may not have mapped, \
         bound a keyboard or finished its restore, which is the lost-first-keystroke failure \
         `window_renders_typed_text_under_cage` documents as signature (a); it passes or fails \
         with the host's load. Add `{READY_CALL_NEEDLE} <died-code> \"<label>\"` straight after \
         `app_pid=$!`:\n{}",
        missing_wait.len(),
        missing_wait.join("\n")
    );
    assert!(
        sleep_before_wait.is_empty(),
        "{} sleep(s) sit between a launch and its readiness wait. That is a fixed launch \
         budget standing in for `wait_ready`: on an idle host it is time wasted, and it implies \
         a margin the readiness file is meant to make unnecessary. Delete the sleep; if a first \
         key is still lost, the app's readiness predicate lacks a condition:\n{}",
        sleep_before_wait.len(),
        sleep_before_wait.join("\n")
    );

    assert!(
        capture_not_followed_by_wait.is_empty(),
        "{} launch(es) run a command between capturing the pid and `{READY_CALL_NEEDLE}`. The \
         first-input scan above stops only at a line that itself starts with wtype, hsend, hstep \
         or grim, so a scenario helper that wraps input (fill_buffer, probe_winnr, wv_chord, \
         quit_app, ...) placed here sends keys or a capture the scan never sees, into a window \
         that may not accept keystrokes yet. Move `{READY_CALL_NEEDLE} <died-code> \"<label>\"` \
         to the first command after `{PID_CAPTURE_LINE}`; only blank and comment lines may sit \
         between them:\n{}",
        capture_not_followed_by_wait.len(),
        capture_not_followed_by_wait.join("\n")
    );

    assert_eq!(
        definitions.len(),
        1,
        "expected exactly one line starting `{READY_DEF_NEEDLE}` in this binary's sources, the \
         one in READINESS_PRELUDE; found it at {definitions:?}. Zero means the prelude's \
         definition was renamed or reformatted, so this check no longer sees it (and a \
         scenario calling `wait_ready` would exit 127). More than one means a scenario defines \
         its own, which shadows the prelude's for that scenario and need not wait for \
         WOLF_READY_FILE at all."
    );
    let prelude_definitions = READINESS_PRELUDE
        .lines()
        .filter(|line| line.trim().starts_with(READY_DEF_NEEDLE))
        .count();
    assert_eq!(
        prelude_definitions, 1,
        "the one `{READY_DEF_NEEDLE}` definition ({definitions:?}) must be the one in \
         READINESS_PRELUDE, which cage_command prepends to every scenario; READINESS_PRELUDE \
         holds {prelude_definitions}. A definition living in a single scenario leaves every \
         other scenario's `wait_ready` call undefined (exit 127)."
    );

    assert!(
        old_probes.is_empty(),
        "the bare launch-probe loop line `{OLD_PROBE_LINE}` appears at: {}. That loop \
         returns as soon as cage accepts a virtual keyboard, which is before the window maps, \
         and leaves the scenario to rely on a fixed settle. Use `{READY_CALL_NEEDLE} \
         <died-code> \"<label>\"` instead; the prelude's `wait_ready` already primes the \
         seat with a key-free `wtype \"\"`.",
        old_probes.join(", ")
    );
}

/// [`SMOKE_APPEARANCE`] re-typed here as a LITERAL, per this binary's
/// re-declare-do-not-import discipline (the `config_seam` tests'
/// `EXPECT_FALLBACK_FONT_PT` precedent).
///
/// Reading the const and comparing it to itself would hold for whatever value it
/// took, including the one that inverts every OCR page in this binary. Stating
/// `dark` independently means a change to the shared pin fails HERE, loudly and in
/// microseconds, rather than at every OCR assertion later.
const EXPECT_SMOKE_APPEARANCE: &str = "dark";

/// Enforces mechanically that the SHARED seeded `wolf-config.yaml` — the one
/// [`cage_command`] hands every smoke — pins `appearance` to `dark`.
///
/// ## The premise, and why it needs a guard rather than a comment
///
/// Every OCR smoke reads a light-on-dark grid, which is what
/// `crates/wolf-gnome/tests/README.md` records as the premise for sparse-text
/// segmentation. That grid is the captive Neovim's default colorscheme responding
/// to `&background`, and the front-end pushes `&background` from the configured
/// `appearance`. So the seeded value sits upstream of every OCR
/// assertion at once — and it is written in [`write_smoke_config`], in a
/// different file from every assertion it can disarm. That distance is the same reason the
/// fixture-name guards exist.
///
/// ## The two ways the premise dies, and both are covered
///
/// **The key stops being emitted.** `appearance` is the one seeded field
/// `wolf_core`'s `ConfigSection` does NOT require: it carries a
/// `#[serde(default)]` of `auto`, so an omitted key is not a parse error and not a
/// fallback to `default_config()` either. The file still loads, every font field
/// still applies, and the appearance silently becomes `auto` — resolved against a
/// settings portal that does not exist under `cage`. Nothing else in this binary
/// would notice.
///
/// **The value is re-pinned.** A containment check alone cannot see that, since
/// `appearance: light` is just as present as `appearance: dark`. Hence the literal
/// [`EXPECT_SMOKE_APPEARANCE`] rather than a comparison against the const itself.
///
/// ## Falsifiers
///
/// Delete the ` appearance: {appearance}\n` fragment from the `format!` in
/// [`write_smoke_config`] — assertion 2 fires. Re-pin [`SMOKE_APPEARANCE`] to
/// `"light"` — assertion 1 fires, ahead of the OCR smokes that would otherwise
/// invert together with no single failure naming the cause.
///
/// ## What it needs
///
/// NO environment: no compositor, no `require_smoke_env`, no cage slot, no
/// subprocess, and — unlike
/// [`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`] — no pinned
/// `FONTCONFIG_FILE`, because it measures no cell. It writes into a fresh temp
/// directory through the same [`smoke_config_dir`] every smoke's `cage_command`
/// call goes through, so what it reads back is the artifact itself rather than a
/// re-derivation of it.
#[test]
fn the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes() {
    const GUARD: &str = "the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes";

    // --- Assertion 1: the VALUE, stated independently -------------------------
    assert_eq!(
        SMOKE_APPEARANCE, EXPECT_SMOKE_APPEARANCE,
        "{GUARD}: SMOKE_APPEARANCE is {SMOKE_APPEARANCE:?}, but every OCR smoke in this binary is \
         calibrated on a light-on-dark grid and {EXPECT_SMOKE_APPEARANCE:?} is what produces one. \
         It is also what Neovim already defaults to under `--embed`, which is why pinning it moves \
         no page at all. Re-pinning to \"light\" inverts every OCR page simultaneously, \
         and no single one of their failures would name this const as the cause — which is exactly \
         why the expected value is re-typed here instead of read from the pin."
    );

    // Assertion 2: the seeded file carries it, read through `smoke_config_dir` like `cage_command`.
    let cwd = make_temp_cwd();
    let dir = smoke_config_dir(&cwd);
    let path = dir.join(CONFIG_FILE_NAME);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{GUARD}: could not read the shared seeded config {} that `smoke_config_dir` just \
             wrote: {e}. That helper is infallible by construction (it panics on a failed write), \
             so an unreadable file here means the path it returned is not the path it wrote.",
            path.display()
        )
    });
    let key = format!("appearance: {EXPECT_SMOKE_APPEARANCE}");
    assert!(
        text.contains(&key),
        "{GUARD}: the shared seeded config {} does not carry {key:?}. Its contents were:\n{text}\n\
         `appearance` is the one seeded field `wolf_core`'s ConfigSection does NOT require — it \
         has a `#[serde(default)]` of `auto` — so an omitted key does NOT fail the parse and does \
         NOT fall back to `default_config()`. The file loads, every font field applies, and the \
         appearance silently becomes `auto`, resolved against a settings portal that does not \
         exist under `cage`. Every OCR smoke would then depend on the compositor's absent theme \
         state. Restore the `appearance: {{appearance}}` line in `write_smoke_config`'s `format!`.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Pointer smokes: shared Rust helpers.

/// The monospace family the app measures its cell metrics with.
///
/// This is the family [`smoke_config_dir`] SEEDS into every smoke's own
/// `wolf-config.yaml`, so the const and the value the app reads live in one file
/// and cannot drift apart. `crates/wolf-gnome/tests/paint_golden.rs` carries the
/// same local const because it measures headlessly and seeds no config.
///
/// It is not the family `wolf_core::default_config()` names. That one is
/// `iA Writer Mono S`, which the pinned `FONTCONFIG_FILE` also resolves — so a
/// smoke whose config went missing would render legibly in the WRONG typeface
/// rather than failing to render at all. If this const and the seeded file ever
/// disagree, [`cell_metrics_for_smoke`] silently measures a different cell size,
/// every derived pixel target lands in the wrong cell, and the exact-delta
/// assertions are what catch it — which is why their failure messages name this
/// const.
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

/// [`SMOKE_LINE_HEIGHT`] as an `f64` — PARSED, never re-typed as a literal, for the
/// same reason [`smoke_font_pt`] is.
///
/// The app reads that same STRING out of the `wolf-config.yaml`
/// [`smoke_config_dir`] seeds, and this file feeds the parsed value to its own
/// `CellMetrics::for_font` call. A re-typed `118.0` here would keep compiling after
/// someone changed the const, and every pixel target in every pointer smoke would
/// then be computed against a cell height the app does not use.
fn smoke_line_height() -> f64 {
    SMOKE_LINE_HEIGHT.parse::<f64>().unwrap_or_else(|e| {
        panic!(
            "SMOKE_LINE_HEIGHT = {SMOKE_LINE_HEIGHT:?} does not parse as an f64 ({e}). It is \
             written verbatim into every smoke's seeded wolf-config.yaml and parsed here to size \
             this test's own cell measurement; the two MUST be the same number, so it has to parse."
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
/// the same family, the same points, the same line-height, and the same
/// `FONTCONFIG_FILE`-pinned DejaVu. `tests/paint_golden.rs::probe_metrics` uses the
/// identical idiom.
///
/// All three inputs reach the app through [`cage_command`]: the family and the
/// line-height through the `wolf-config.yaml` [`smoke_config_dir`] seeds, the
/// points through `WOLF_FONT_PT`, which wins over the file's `font-size`.
fn cell_metrics_for_smoke() -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1)
        .expect("failed to create the 1x1 probe surface for cell measurement");
    let cr = Context::new(&probe).expect("failed to create the probe Cairo context");
    let metrics =
        CellMetrics::for_font(&cr, SMOKE_FONT_FAMILY, smoke_font_pt(), smoke_line_height());
    drop(cr);
    assert!(
        metrics.cell_w > 0.0 && metrics.cell_h > 0.0,
        "CellMetrics::for_font({SMOKE_FONT_FAMILY:?}, {}, {}) produced a non-positive cell size \
         (cell_w = {}, cell_h = {}). Every pixel target in every pointer smoke is derived from \
         these two numbers, so nothing below can mean anything. The usual cause is a missing \
         FONTCONFIG_FILE (the flake devShell pins DejaVu Sans Mono through it), which leaves Pango \
         resolving the family to nothing.",
        smoke_font_pt(),
        smoke_line_height(),
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

/// Runs one pointer scenario — [`POINTER_PREAMBLE`] plus `body` — inside `cage` in
/// `runtime`, and returns cage's exit status.
///
/// The per-smoke environment arrives as a SLICE rather than as positional
/// parameters. That is a deliberate lint-shape decision, following the `GridTarget`
/// precedent in `pane_view.rs`: eight more positional parameters would trip
/// `clippy::too_many_arguments`, which is gating here (`cargo clippy --workspace
/// --all-targets -- -D warnings`), and the only honest fix would be an `#[allow]`.
fn run_pointer_scenario(
    body: &str,
    cwd: &Path,
    runtime: &CageRuntime<'_>,
    out_png: &Path,
    extra_env: &[(&str, &OsStr)],
) -> ExitStatus {
    let mut cmd = pointer_command(body, cwd, runtime, out_png);
    for &(key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.status()
        .expect("failed to spawn `cage` for a headless pointer smoke scenario")
}

/// The `cage` command [`run_pointer_scenario`] runs — [`POINTER_PREAMBLE`] plus `body`,
/// with the environment the preamble reads — for a smoke that must chain more onto it
/// than an environment slice carries, such as `pin_terminal_smoke_shell`.
fn pointer_command(body: &str, cwd: &Path, runtime: &CageRuntime<'_>, out_png: &Path) -> Command {
    // Cargo sets this for integration tests of a crate that declares a `[[bin]]`.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let mut cmd = cage_command(&pointer_scenario(body), cwd, runtime);
    cmd.env("WOLF_BIN", bin)
        .env("OUT_PNG", out_png)
        .env("CMD_FIFO", cwd.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", cwd.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(cwd))
        .env("READY_FILE", pointer_ready_path(cwd))
        .env("PROBE_X", PROBE_X.to_string())
        .env("PROBE_Y", PROBE_Y.to_string())
        .env("BTN_LEFT", POINTER_BUTTON_LEFT);
    cmd
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
         Exit 11: the app died before it was ready for input. A LAUNCH failure — no pointer event \
         was ever injected, and nothing below is about pointer input.\n\
         Exit 40: the app stayed up but never wrote its readiness file within `wait_ready`'s \
         budget; the app's `wolf-ready: waiting on …` stderr lines name the unmet condition. \
         Also a launch failure, not a pointer one.\n\
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
    // Exact: `rounded` is integral and range-checked above. `as` only saturates, which is no guarantee.
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
         measured cell size grew (check that WOLF_FONT_PT = {SMOKE_FONT_PT} still reaches the app, \
         and that the wolf-config.yaml `smoke_config_dir` seeds still carries SMOKE_FONT_FAMILY = \
         {SMOKE_FONT_FAMILY:?} and SMOKE_LINE_HEIGHT = {SMOKE_LINE_HEIGHT}). Move PROBE_X/PROBE_Y \
         or shrink the per-smoke cell deltas; do NOT loosen the delta assertion."
    );
}

/// Asserts the probe click moved the cursor off `(1, 1)`, derives the editor pane's
/// on-screen origin from it, and asserts that origin sits inside the measured band.
/// Returns that origin as `(x, y)` in the injected point's own output coordinates;
/// every caller uses it only inside an `assert!` message, and `assert!` formats
/// lazily, so the origin is observable on a FAILING run and never on a green one.
/// Deriving it from a green run instead means reading the smoke's persisted probe
/// readback and crossing it with an independently measured cell — the method
/// `crates/wolf-gnome/tests/README.md` records.
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
         {PANE_ORIGIN_X_EXPECTED} +/- ({PANE_ORIGIN_TOLERANCE_PX} + cell_w). The five candidate \
         causes, in order of likelihood: (1) the PER-PANE CHROME changed — \
         `editor_inset::EDITOR_PADDING_PX` or the statusbar strip's height — since this origin is \
         the TEXT GRID's corner, which sits inside the pane widget by exactly that chrome; (2) a \
         GTK4 client-side-decoration / shadow-margin offset \
         has appeared under cage (there was none: `motion 640 360` was measured arriving as \
         surface-local `640.00000000, 360.00000000`); (3) the `adw::HeaderBar` height changed — a \
         libadwaita/GTK bump; (4) `window::TREE_PANE_WIDTH` or the `gtk::Paned` divider width \
         changed; (5) the cage output size changed. Derived origin ({origin_x}, {origin_y}); \
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
         {PANE_ORIGIN_Y_EXPECTED} +/- ({PANE_ORIGIN_TOLERANCE_PX} + cell_h). Same five candidate \
         causes as the X band, in the same order, but a Y-only shift points hardest at (1) and (3) \
         — the STATUSBAR STRIP's height and the `adw::HeaderBar`'s — because those two are the \
         whole of what sits above the grid, and only the strip is ours. The strip is a value: \
         `statusbar_layout(metrics.size_px, secondary_font_size(..), ..)`, so a font-size change \
         moves it too. Derived \
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

/// The fields of a persisted leaf the smokes read.
struct LeafFields<'a> {
    /// The buffer path, or the source path of a preview, or a terminal's directory.
    path: &'a str,
    preview: bool,
    terminal: bool,
}

/// A persisted leaf's [`LeafFields`] — `None` for a split.
///
/// The one place the smokes destructure `PaneNode::Leaf`, so a field added to it
/// touches this function and no smoke.
fn leaf_fields(node: &PaneNode) -> Option<LeafFields<'_>> {
    match node {
        PaneNode::Leaf {
            path,
            preview,
            terminal,
            ..
        } => Some(LeafFields {
            path,
            preview: *preview,
            terminal: *terminal,
        }),
        PaneNode::Split { .. } => None,
    }
}

/// The file NAME of a persisted leaf's path — `None` for a split, and `None` for a
/// leaf whose path is empty or has no final component.
fn leaf_tail(node: &PaneNode) -> Option<&str> {
    leaf_fields(node).and_then(|leaf| Path::new(leaf.path).file_name().and_then(OsStr::to_str))
}

/// The buffer NAMES a `tabpagebuflist()` readback reports, SORTED.
///
/// The scenario joins them with a COMMA rather than a space, because a preview
/// scratch's own name carries one (`Preview: <cwd-relative path>`), and this sorts
/// them so the caller asserts on the SET of buffers rather than on nvim's window
/// order — which for a `:vsplit` depends on `'splitright'` and is not what any
/// assertion here is about.
///
/// WHICH name each scenario writes is the scenario's own choice, not this function's:
/// most reduce it to the tail with `:t`, while `POINTER_LINK_BODY` writes
/// `bufname(b)` raw because its fixtures live in a subdirectory a tail would drop.
/// This only splits, trims and sorts.
fn read_window_buffer_names(path: &Path, smoke: &str) -> Vec<String> {
    let raw = read_readback(path, smoke);
    let mut names: Vec<String> = raw
        .trim()
        .split(',')
        .map(|name| name.trim().to_string())
        .collect();
    assert!(
        names.iter().all(|name| !name.is_empty()),
        "{smoke}: the window-buffer readback {} holds an EMPTY name among {names:?}. Raw contents \
         were {raw:?}. An empty entry means a window is bound to an unnamed buffer, or that the \
         typed `writefile` produced no list at all.",
        path.display()
    );
    names.sort();
    names
}

/// Whether a persisted leaf is a Markdown PREVIEW — `None` for a split.
///
/// Read beside [`leaf_tail`] rather than folded into it: the two answer different
/// questions about one leaf, and a `(tail, preview)` pair is what
/// `preview_pane_renders_and_edit_source_returns_under_cage` compares as a set.
fn leaf_preview(node: &PaneNode) -> Option<bool> {
    leaf_fields(node).map(|leaf| leaf.preview)
}

/// The fifth compositor-free guard: `editor_inset`'s content-width cap CANNOT bind
/// at the geometry the OCR smokes run at, which is the unstated premise every one of
/// them rests on.
///
/// ## Why a premise needs a guard
///
/// `editor_inset::grid_rect` caps a pane's grid at
/// `wolf_gnome::editor_inset::MAX_GRID_COLS` columns and CENTRES what is left, so a
/// pane wide enough for the cap draws its first glyph at
/// `EDITOR_PADDING_PX + (inset_w - cap_w) / 2` instead of at `EDITOR_PADDING_PX`.
/// Every pointer smoke's derived origin, every derived click target, and the position
/// of every glyph on every OCR page assume that offset is zero.
///
/// It is zero today by arithmetic and not by design: one pane at the pinned output
/// size leaves 989 px of grid, while 90 columns of the measured 28 pt cell need
/// 2022.89 px. The margin is a factor of two, so this suite's own settings cannot reach it —
/// but the font size, the output size and the header bar's width are all things a
/// later change touches routinely, and the first symptom of the cap binding would be
/// every OCR assertion failing at once for no visible reason.
///
/// ## What this guard reads, and what it therefore needs
///
/// The pane area's width from [`CAGE_OUTPUT_W_EXPECTED`] and
/// [`OVERLAY_ORIGIN_X_EXPECTED`], the padding and the cap from the production
/// constants, and the cell from [`cell_metrics_for_smoke`] — the same
/// `CellMetrics::for_font` call the app itself makes.
///
/// That last one is why this guard, ALONE among the guards, needs the devShell's pinned
/// `FONTCONFIG_FILE`: with no pinned DejaVu, Pango resolves the family to something
/// else and the cell width is not the one the app measures.
/// [`cell_metrics_for_smoke`] fails loudly on a non-positive cell, so an
/// unprovisioned host fails there rather than here. It still needs no compositor, no
/// [`require_smoke_env`], no cage slot and no subprocess.
///
/// ## What it does not cover
///
/// One pane only. A split makes each pane NARROWER, so it can only widen the margin,
/// and no smoke ever gives a pane more room than an unsplit one has.
///
/// FALSIFYING EDIT: set `editor_inset::MAX_GRID_COLS` to 20. Then
/// `20 * 22.4765625 = 449.5` px fits inside 989 px, the cap binds in every smoke, and
/// this guard fires. MEASURED, and the run's own live numbers are the point of the
/// record: `One pane leaves 989 px of grid (1029 px of pane area, less two 20 px
/// paddings), while 20 columns of the measured 22.4765625 px cell need only
/// 449.53125 px`. That 22.4765625 is the cell [`cell_metrics_for_smoke`] really
/// produced on the measuring host, which is what makes this a comparison against a
/// real cell rather than between two literals.
#[test]
fn the_column_cap_cannot_bind_at_the_ocr_smokes_geometry() {
    let cell = cell_metrics_for_smoke();

    // The pane area's width, as OVERLAY_ORIGIN_X_EXPECTED measures; not derived from
    // PANE_ORIGIN_X_EXPECTED, which is the text grid's corner, not the widget's.
    let pane_area_w =
        exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED") - OVERLAY_ORIGIN_X_EXPECTED;
    let inset_w = pane_area_w - 2.0 * wolf_gnome::editor_inset::EDITOR_PADDING_PX;
    let cap_w = f64::from(wolf_gnome::editor_inset::MAX_GRID_COLS) * cell.cell_w;

    // Non-vacuity: a non-positive inset satisfies the comparison for any cap, and means the two pinned
    // constants crossed.
    assert!(
        inset_w > 0.0,
        "the pane area derived from CAGE_OUTPUT_W_EXPECTED ({CAGE_OUTPUT_W_EXPECTED}) and \
         OVERLAY_ORIGIN_X_EXPECTED ({OVERLAY_ORIGIN_X_EXPECTED}) leaves {inset_w} px of grid once \
         both editor paddings are removed. A non-positive width means those two constants have \
         crossed, at which point the cap comparison below holds for ANY cap and proves nothing. \
         Re-measure the overlay origin before reading anything else in this binary."
    );

    assert!(
        inset_w < cap_w,
        "THE COLUMN CAP HAS STARTED TO BIND IN THE OCR SMOKES. One pane leaves {inset_w} px of \
         grid ({pane_area_w} px of pane area, less two {} px paddings), while {} columns of the \
         measured {} px cell need only {cap_w} px — so `editor_inset::grid_rect` caps that \
         grid and CENTRES it, moving every glyph in every capture right by half the difference. \
         What that means, in order: every OCR assertion is reading a page laid out differently \
         from the one it was calibrated against; PANE_ORIGIN_X_EXPECTED = \
         {PANE_ORIGIN_X_EXPECTED} is stale, because the text grid starts at the centred origin \
         rather than at the padding; and every pointer smoke's derived click target moves with it. DO NOT relax this guard and \
         DO NOT widen PANE_ORIGIN_TOLERANCE_PX to absorb it — re-derive the pane origin from a \
         probe click the way its own doc comment describes, and decide deliberately whether the \
         smokes should keep running at a size where the cap binds. Likely causes, in order: \
         SMOKE_FONT_PT was lowered (a smaller cell fits more columns in the same pane), the cage \
         output was widened, the sidebar narrowed, or MAX_GRID_COLS was raised.",
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
        wolf_gnome::editor_inset::MAX_GRID_COLS,
        cell.cell_w
    );
}

/// Writes the markdown fixtures a smoke owns into `cwd`, plus the
/// [`SIDEBAR_FILLER_FILES`] page-density fixture. Most of those names are
/// `RELAUNCH_FIXTURE_NAMES` rows; the ones deliberately absent from that table
/// carry their exemption at their own const, so do not read this helper as a
/// guarantee that every file it writes is registered there.
///
/// Each `contents` is a whole file, newline-terminated here rather than by the
/// caller. Most callers pass a bare heading; the divider smoke's right-hand fixture
/// passes a heading plus a body (`divider_right_fixture_body`), and the
/// markdown-preview smoke's source passes a heading plus a paragraph, whose density
/// is for the WEB VIEW rather than for the editor grid.
///
/// The fillers are FIXTURE, not assertion: the asserted terms in both relaunch
/// smokes are sparse, light-on-dark EDITOR text, which is exactly the case
/// [`SIDEBAR_FILLER_COUNT`] documents — below the measured five-file floor
/// `tesseract` misclassifies isolated glyphs on a nearly-blank page. Nothing
/// asserts on a filler, and
/// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` is what keeps one
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

/// A pixel's colour as `(r, g, b)`. No alpha: see [`decode_capture`].
type Rgb8 = (u8, u8, u8);

/// Bytes per pixel in a decoded `grim` capture. Four even for
/// [`cairo::Format::Rgb24`], whose fourth byte is PADDING — see [`decode_capture`].
const CAPTURE_BYTES_PER_PIXEL: usize = 4;

/// How far inside a pane's own left and right edges the sampled columns start.
///
/// It clears three things at once, and names them rather than its number. First, half
/// of the `gtk::Paned` handle between the two panes — this binary deliberately never
/// models that handle's width, so the boundary between the two halves is known only
/// to within it. Second, the pane's own `editor_inset::EDITOR_PADDING_PX` gutter.
/// Third, the WebKit scrollbar a document taller than its viewport would draw down
/// the view's right edge (this fixture is far shorter than that, so the clearance is
/// belt-and-braces there).
///
/// FOUR paddings, because the padding is the only one of the three whose size this
/// file knows: at the pinned output that is 80 px, against a GTK handle of
/// single-digit pixels and a scrollbar of roughly fifteen. Derived values at
/// 1280x720: the preview band spans columns 846..1200 and the editor band 331..686.
const BAND_X_INSET_PX: f64 = 4.0 * wolf_gnome::editor_inset::EDITOR_PADDING_PX;

/// How far above the output's bottom edge the sampled rows stop.
///
/// It clears the window's full-width command-line strip, the pane's own bottom
/// `EDITOR_PADDING_PX` gutter, and the message toast that would sit above the strip
/// if the captive Neovim emitted one. SIX paddings — 120 px at the pinned output —
/// against a strip one grid row tall (44 px at [`SMOKE_FONT_PT`]) plus that 20 px
/// gutter. Derived value: the sampled rows end at y = 600.
const BAND_BOTTOM_CLEARANCE_PX: f64 = 6.0 * wolf_gnome::editor_inset::EDITOR_PADDING_PX;

/// A rectangle of output pixels this smoke samples, in absolute compositor
/// coordinates. Half-open on both axes: `x0 <= x < x1`, `y0 <= y < y1`.
///
/// A struct rather than four loose `usize`s because the samplers below would
/// otherwise take five and six arguments — `clippy::too_many_arguments` territory —
/// and because four same-typed positional arguments are exactly the shape a later
/// edit transposes silently.
#[derive(Clone, Copy, Debug)]
struct SampleRect {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

impl SampleRect {
    /// How many pixels the rectangle covers.
    fn pixels(self) -> usize {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }
}

/// A decoded `grim` capture: its pixels COPIED out of the cairo surface, plus the
/// three numbers needed to address them.
struct Capture {
    /// The surface's bytes, owned. See [`decode_capture`] for why they are copied.
    data: Vec<u8>,
    /// Bytes per row, which is NOT `width * 4` in general.
    stride: usize,
    /// The capture's width in pixels.
    width: usize,
    /// The capture's height in pixels.
    height: usize,
}

impl Capture {
    /// The `(r, g, b)` at `(x, y)`.
    ///
    /// Little-endian byte order inside a pixel is `B, G, R, X`. The fourth byte is
    /// read by NOTHING here — see [`decode_capture`].
    fn pixel(&self, x: usize, y: usize) -> Rgb8 {
        let i = y * self.stride + x * CAPTURE_BYTES_PER_PIXEL;
        (self.data[i + 2], self.data[i + 1], self.data[i])
    }

    /// How many pixels inside `rect` are exactly `colour`.
    fn count_exactly(&self, rect: SampleRect, colour: Rgb8) -> usize {
        let mut hits = 0;
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                if self.pixel(x, y) == colour {
                    hits += 1;
                }
            }
        }
        hits
    }

    /// The first row in `rows` every one of whose sampled columns is exactly
    /// `colour`, or `None` if there is none.
    ///
    /// This is how assertion 6 locates the web view's own top edge without modelling
    /// the per-pane chrome above it. EVERY column has to match, so a row of statusbar
    /// text — which is mostly the editor's background with glyphs through it — can
    /// never be mistaken for the first row of the document. A stray pixel in the web
    /// view's own first row costs one row of the band and nothing more.
    fn first_uniform_row(
        &self,
        columns: (usize, usize),
        rows: (usize, usize),
        colour: Rgb8,
    ) -> Option<usize> {
        let (x0, x1) = columns;
        let (y0, y1) = rows;
        (y0..y1).find(|&y| (x0..x1).all(|x| self.pixel(x, y) == colour))
    }

    /// The first COLUMN inside `rect` every one of whose sampled rows is exactly
    /// `colour`, or `None` if there is none.
    ///
    /// [`Self::first_uniform_row`]'s transpose, and it exists for the same reason:
    /// `file_picker_covers_the_live_preview_under_cage` has to locate a VERTICAL
    /// edge — the web view's own left edge — without modelling the chrome to either
    /// side of it. EVERY row has to match, so a column of statusbar or list-row text
    /// can never be mistaken for the first column of a background.
    ///
    /// It takes a [`SampleRect`] rather than the two tuples its row-wise sibling
    /// takes, per the rule on that struct: `x0..x1` is the range scanned and `y0..y1`
    /// the span sampled, and a transposed pair of tuples would compile and search the
    /// wrong axis.
    ///
    /// It locates a boundary by the colour that ARRIVES, which names the edge exactly
    /// only where the two regions meet with nothing between them. The far edge of a
    /// surface that carries a shadow or a rounded corner is
    /// [`Self::first_column_past_surface`]'s job instead.
    fn first_uniform_column(&self, rect: SampleRect, colour: Rgb8) -> Option<usize> {
        (rect.x0..rect.x1).find(|&x| (rect.y0..rect.y1).all(|y| self.pixel(x, y) == colour))
    }

    /// The first COLUMN inside `rect` at least `min_off` of whose sampled rows are NOT
    /// `colour`, or `None` if there is none.
    ///
    /// The far edge of a filled surface, located from the SURFACE side rather than from
    /// whatever lies beyond it. [`Self::first_uniform_column`] locates an edge by the
    /// colour that arrives, and an outset `box-shadow` is a band of NEITHER colour: a
    /// scan for the far side's exact value walks through the shadow and reports an edge
    /// several columns past the surface it was asked about, which is a rectangle that
    /// silently contains pixels the surface never painted.
    ///
    /// `min_off` is what keeps the formulation usable on a surface with ROUNDED corners,
    /// where a line near the edge is mostly the surface and partly not. A threshold of
    /// one would stop at the first line the corner's arc touches — and on a scan that
    /// starts inside those lines, that is the scan's own first line, so the located edge
    /// would sit at the scan's start whatever the surface did. See
    /// `PICKOVER_EDGE_MIN_NON_FILL_PX` for the measured separation its one caller
    /// relies on.
    fn first_column_past_surface(
        &self,
        rect: SampleRect,
        colour: Rgb8,
        min_off: usize,
    ) -> Option<usize> {
        (rect.x0..rect.x1).find(|&x| {
            (rect.y0..rect.y1)
                .filter(|&y| self.pixel(x, y) != colour)
                .count()
                >= min_off
        })
    }

    /// [`Self::first_column_past_surface`]'s transpose: the first ROW in `rows` at least
    /// `min_off` of whose sampled `columns` are NOT `colour`, or `None` if there is
    /// none. That doc comment carries the reasoning for both.
    ///
    /// It takes the two tuples [`Self::first_uniform_row`] takes rather than a
    /// [`SampleRect`], so the two row-wise scans read alike at their call sites: `rows`
    /// is the range scanned and `columns` the span sampled.
    ///
    /// An EMPTY `columns` span makes every row read zero not-matching pixels, so the
    /// scan runs off the end of `rows` and answers `None` rather than answering its own
    /// first row. The caller establishes a non-empty span before calling — see assertion
    /// 6 of `file_picker_covers_the_live_preview_under_cage`, which is ordered ahead of
    /// this scan for exactly that reason.
    fn first_row_past_surface(
        &self,
        columns: (usize, usize),
        rows: (usize, usize),
        colour: Rgb8,
        min_off: usize,
    ) -> Option<usize> {
        let (x0, x1) = columns;
        let (y0, y1) = rows;
        (y0..y1).find(|&y| (x0..x1).filter(|&x| self.pixel(x, y) != colour).count() >= min_off)
    }
}

/// `count / total`, with both sides widened to `f64` without an `as` cast.
///
/// `u32` is the intermediate because `f64::from(u32)` is lossless and a sampled band
/// is tens of thousands of pixels, so the narrowing can only fail on a rect that is
/// already nonsense.
fn band_fraction(count: usize, total: usize, what: &str) -> f64 {
    assert!(
        total > 0,
        "{what}: the sampled band is EMPTY, so every fraction computed from it would be a division \
         by zero and every threshold below would be vacuous. Re-derive the band's rectangle from \
         CAGE_OUTPUT_*_EXPECTED and OVERLAY_ORIGIN_*_EXPECTED."
    );
    let numerator = u32::try_from(count)
        .unwrap_or_else(|e| panic!("{what}: pixel count {count} does not fit in a u32 ({e})"));
    let denominator = u32::try_from(total)
        .unwrap_or_else(|e| panic!("{what}: band size {total} does not fit in a u32 ({e})"));
    f64::from(numerator) / f64::from(denominator)
}

/// Rounds a derived pixel coordinate to the `usize` index [`Capture::pixel`]
/// addresses rows and columns with, through [`pixel_target`]'s finite-and-in-range
/// check — never a bare `as` cast.
fn pixel_index(value: f64, what: &str) -> usize {
    let rounded = pixel_target(value, what);
    usize::try_from(rounded).unwrap_or_else(|e| {
        panic!(
            "{what} = {rounded} is not a usable pixel index ({e}). `pixel_target` has already \
             proved it non-negative and finite, so this is unreachable unless that bound changed."
        )
    })
}

/// Decodes one `grim` capture into owned pixels, checking its format and size first.
///
/// ## Two PNG facts a later reader will "simplify" back, both MEASURED
///
/// **The decoded format is [`cairo::Format::Rgb24`], NOT `ARgb32`.** `grim` writes an
/// OPAQUE PNG, so cairo picks the format with no alpha channel. A decoder written
/// against `paint_golden.rs`'s in-memory surfaces — which really are `ARgb32` —
/// asserts the wrong one and fails on a perfectly good capture.
///
/// **The little-endian byte order inside a pixel is `B, G, R, X`, where the fourth
/// byte is PADDING and not alpha.** Cairo keeps the 32-bit stride for `Rgb24`, so the
/// arithmetic is the same as `ARgb32`'s and only the meaning of the last byte
/// changes. Its contents are UNDEFINED, so nothing here may read it — which is why
/// [`Rgb8`] carries no alpha component and why every comparison in this smoke is on
/// three channels.
///
/// ## Why the bytes are copied
///
/// `ImageSurface::data` hands out a borrow of the live surface and refuses while any
/// other reference exists. Copying into an owned `Vec` and dropping the surface here
/// means no caller has to keep the surface alive alongside its samples, and no
/// assertion below can fail to compile for a borrow reason that has nothing to do
/// with what it is measuring.
///
/// This is also ASSERTION 2 of
/// `preview_document_follows_the_configured_appearance_under_cage`: the size check
/// is what makes every derived rectangle in this section mean what it says.
fn decode_capture(path: &Path, smoke: &str, label: &str) -> Capture {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| {
        panic!(
            "{smoke}: could not open {label}'s capture {}: {e}. The scenario runs `grim` under \
             `set -eu`, so a cage status of 0 with no file here means grim wrote somewhere else.",
            path.display()
        )
    });
    let mut surface = ImageSurface::create_from_png(&mut file).unwrap_or_else(|e| {
        panic!(
            "{smoke}: cairo could not decode {label}'s capture {} as a PNG: {e}. A zero-length \
             file means `grim` produced no image data; a decode error on a non-empty file means \
             it is not a PNG at all.",
            path.display()
        )
    });
    let format = surface.format();
    assert!(
        matches!(format, Format::Rgb24),
        "{smoke}: {label}'s capture {} decoded as {format:?}, not Rgb24. `grim` writes an OPAQUE \
         PNG and cairo picks the format with no alpha channel for one, so Rgb24 is what a real \
         capture gives. An ARgb32 here means the file did not come from `grim`. Do NOT relax this \
         to accept both: the per-pixel byte layout this smoke reads is written down against Rgb24 \
         in `decode_capture`'s doc comment, and its fourth byte is PADDING rather than alpha.",
        path.display()
    );
    let width = usize::try_from(surface.width())
        .unwrap_or_else(|e| panic!("{smoke}: {label}'s capture has a negative width ({e})"));
    let height = usize::try_from(surface.height())
        .unwrap_or_else(|e| panic!("{smoke}: {label}'s capture has a negative height ({e})"));
    let stride = usize::try_from(surface.stride())
        .unwrap_or_else(|e| panic!("{smoke}: {label}'s capture has a negative stride ({e})"));

    let want_w = usize::try_from(CAGE_OUTPUT_W_EXPECTED)
        .expect("CAGE_OUTPUT_W_EXPECTED is a small positive literal");
    let want_h = usize::try_from(CAGE_OUTPUT_H_EXPECTED)
        .expect("CAGE_OUTPUT_H_EXPECTED is a small positive literal");
    assert!(
        width == want_w && height == want_h,
        "{smoke}: {label}'s capture {} is {width}x{height}, not the {want_w}x{want_h} this binary \
         pins as the cage output. EVERY sampling rectangle in this smoke is derived from those two \
         numbers plus OVERLAY_ORIGIN_*_EXPECTED, so a different output size does not merely shift \
         the bands — it aims them at panes that are not where they were measured, and assertions \
         4, 5 and 6 would all be reading the wrong region while still producing plausible \
         percentages. Re-measure the output and the overlay origin together; do not adjust one.",
        path.display()
    );
    assert!(
        stride >= width * CAPTURE_BYTES_PER_PIXEL,
        "{smoke}: {label}'s capture reports a stride of {stride} bytes for {width} pixels, which \
         is less than the {} its own format needs. Every pixel read would then run into the \
         previous row. This is a cairo invariant, so a failure here means the surface is not the \
         image surface it claims to be.",
        width * CAPTURE_BYTES_PER_PIXEL
    );

    let data = surface
        .data()
        .unwrap_or_else(|e| {
            panic!(
                "{smoke}: cairo refused to lend {label}'s decoded pixels ({e}). That happens when \
                 another reference to the surface is still alive, which cannot be the case here."
            )
        })
        .to_vec();
    drop(surface);

    Capture {
        data,
        stride,
        width,
        height,
    }
}

/// Runs `tesseract <png> stdout --psm 11` and returns its lowercased text output.
/// (`--psm 11` — "sparse text" — is needed because the default page-segmentation
/// mode returns empty on the sparse, light-on-dark editor grid.) Panics with the
/// captured stderr if tesseract fails to run or exits non-zero.
///
/// This is the reader every smoke but one uses, and its mode is deliberately fixed
/// rather than a parameter. The one exception reads the page a SECOND time through
/// `ocr_uniform_block_to_lowercase`; that function's doc says why, and nothing
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

/// Crops `band` out of an already-taken capture, writes the crop to `band_png`, and
/// returns `tesseract <band_png> stdout --psm 11` lowercased.
///
/// A THIRD reader, used by `terminal_drawer_opens_and_is_not_persisted_under_cage`
/// and by the project-search smokes,
/// `search_panel_streams_results_and_opens_the_match_under_cage` and
/// `search_chord_reveals_the_sidebar_and_escape_returns_to_the_editor_under_cage`.
/// Those two crop for a different reason than the drawer's: their claim is WHICH
/// half of the window draws a search result, the sidebar or the pane area, and a
/// whole-page reading cannot say where on the page a word was. Their bands are
/// `projsearch_sidebar_band` and `projsearch_pane_band`, and their `--psm 11` at
/// those bands is NOT measured; a misread there is first a question for the crop.
///
/// ## Why it crops instead of taking a second `grim`
///
/// The sample is limited to the band below the drawer's statusbar, the region
/// measured stable across runs — `drawer_band_rect` owns that derivation and the
/// md5 evidence behind it. Doing the limiting with a second, region-limited
/// `grim` would sample a DIFFERENT INSTANT: the marker leg's capture is deliberately
/// taken right after the marker line runs, before any readback scrolls the one-row
/// drawer, and a second capture would be after several more keystrokes. Cropping the
/// retained PNG keeps every assertion about one frame.
///
/// The crop is RETAINED as its own file rather than being a temporary, because a
/// failing band assertion is unreadable without it: the panic prints the OCR text,
/// and the only way to tell "the band is blank" from "tesseract misread a marker
/// that is plainly there" is to look at the pixels that were fed to it.
///
/// ## Why it is a separate function rather than a mode/rect parameter on
/// [`ocr_to_lowercase`]
///
/// The same reason `ocr_uniform_block_to_lowercase` gives, and it applies with more
/// force to a RECTANGLE than to a mode: an optional crop on the shared reader is a
/// value a later change can pass wrongly from a call site that looks unrelated, and
/// the symptom would be a distant smoke reading a fraction of its own page while
/// still finding enough of its term to pass. The reader every other OCR assertion
/// depends on stays a fixed, unparameterized command over a whole file.
///
/// The `--psm 11` here is the same sparse-text mode, and it is MEASURED at this band:
/// in a measured run the crop's entire OCR output was the single token
/// `ZZDRAWERMARK]` — the marker, one stray bracket, and no statusbar ink at all, which
/// is the band's placement and the mode's suitability in one reading. If the marker is
/// visibly present in the retained crop and the assertion still fails, `--psm 6` is the
/// first thing to try —
/// `ocr_uniform_block_to_lowercase`'s doc comment carries the precedent, including
/// the measured case where a narrow column read correctly in one mode and not the
/// other.
fn ocr_band_to_lowercase(png: &Path, band: SampleRect, band_png: &Path, smoke: &str) -> String {
    let capture = decode_capture(png, smoke, "the banded capture");

    // Non-vacuity: an empty band panics below or hands tesseract a blank image, which passes every
    // must-not-contain.
    assert!(
        band.x0 < band.x1 && band.y0 < band.y1,
        "{smoke}: the sampled band {band:?} is empty or inverted, so there is nothing to read. \
         Re-derive it from the function that built it rather than adjusting the assertion."
    );
    assert!(
        band.x1 <= capture.width && band.y1 <= capture.height,
        "{smoke}: the sampled band {band:?} runs past the {}x{} capture. Every band is derived \
         from CAGE_OUTPUT_*_EXPECTED, OVERLAY_ORIGIN_*_EXPECTED or a production function, so this \
         means the cage output size has moved and the band is aimed at pixels that do not exist.",
        capture.width,
        capture.height
    );

    let width = i32::try_from(band.x1 - band.x0)
        .unwrap_or_else(|e| panic!("{smoke}: the band's width does not fit in an i32 ({e})"));
    let height = i32::try_from(band.y1 - band.y0)
        .unwrap_or_else(|e| panic!("{smoke}: the band's height does not fit in an i32 ({e})"));
    let mut crop = ImageSurface::create(Format::Rgb24, width, height).unwrap_or_else(|e| {
        panic!("{smoke}: could not create the {width}x{height} band surface: {e}")
    });
    let stride = usize::try_from(crop.stride())
        .unwrap_or_else(|e| panic!("{smoke}: the band surface has a negative stride ({e})"));
    {
        let mut data = crop
            .data()
            .unwrap_or_else(|e| panic!("{smoke}: cairo refused to lend the band's pixels ({e})"));
        for (row, y) in (band.y0..band.y1).enumerate() {
            for (col, x) in (band.x0..band.x1).enumerate() {
                let (r, g, b) = capture.pixel(x, y);
                let i = row * stride + col * CAPTURE_BYTES_PER_PIXEL;
                // Little-endian `B, G, R, X`, as `Capture::pixel` reads it; the padding byte is set so the buffer
                // holds no uninitialized bytes.
                data[i] = b;
                data[i + 1] = g;
                data[i + 2] = r;
                data[i + 3] = 0xff;
            }
        }
    }
    let mut file = std::fs::File::create(band_png).unwrap_or_else(|e| {
        panic!(
            "{smoke}: could not create the band crop {}: {e}",
            band_png.display()
        )
    });
    crop.write_to_png(&mut file).unwrap_or_else(|e| {
        panic!(
            "{smoke}: could not write the band crop {}: {e}. `cairo`'s `png` feature is enabled \
             unconditionally in this crate's Cargo.toml, so this is not a feature-flag problem.",
            band_png.display()
        )
    });
    drop(file);

    let output = Command::new("tesseract")
        .arg(band_png)
        .arg("stdout")
        .arg("--psm")
        .arg("11")
        .output()
        .expect("failed to spawn `tesseract` for OCR of a band crop");
    assert!(
        output.status.success(),
        "{smoke}: tesseract failed on the band crop {} ({}): {}. The crop is written by this binary \
         rather than by `grim`, so a failure here is about the crop — an empty rectangle, or a \
         surface cairo would not encode — and not about what the app rendered.",
        band_png.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}

/// Whether a packaged scenario also removes [`PACKAGED_SCRUB_GTK_RUNTIME`] and
/// [`PACKAGED_SCRUB_FONT_STACK`] — the two lists that a scenario with an unwrapped
/// launch in it has to keep.
///
/// A named pair rather than a `bool`, because a positional boolean at a call site
/// reads as neither of the two things it can mean, and the two arms make two
/// different claims about the artifact.
#[derive(Clone, Copy, Debug)]
enum GtkRuntimeEnv {
    /// The wrapper must supply the GTK runtime — and the package its own font
    /// configuration — because every launch in the scenario is the wrapped store
    /// binary. [`PACKAGED_SCRUB_FONT_STACK`] goes with
    /// [`PACKAGED_SCRUB_GTK_RUNTIME`] on this arm.
    Scrubbed,
    /// The devShell's values are left in place: an UNWRAPPED binary launches in the
    /// same scenario and cannot reach `activate` without the GTK runtime, and the two
    /// launches must resolve the SAME faces or every derived rectangle moves under one
    /// of them. See [`PACKAGED_SCRUB_FONT_STACK`] for the second half.
    Inherited,
}

/// [`cage_command`] plus the environment scrub every packaged smoke needs.
///
/// It goes THROUGH [`cage_command`] rather than building its own invocation, which
/// is mandatory rather than tidy: that helper is the single spawn site
/// [`cage_is_only_spawned_through_the_isolating_helper`] enforces, and it carries
/// `WOLF_STATE_DIR`, `WOLF_CONFIG_DIR`, `HOME`, `XDG_CACHE_HOME`, `GTK_IM_MODULE`,
/// `GSETTINGS_BACKEND`, `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME`, `WOLF_READY_FILE` (with
/// the readiness prelude), the private `XDG_RUNTIME_DIR` of `runtime`, the generated
/// session-bus configuration, and the removal of `DBUS_SESSION_BUS_ADDRESS` and the host display
/// variables. None of those is a devShell export and none of them may be scrubbed:
/// they are the harness's own isolation. The runtime directory is what keeps the
/// store binary from handing its launch off to a copy the developer has installed and
/// running, and the config directory is what pins the font family, size, line-height
/// and appearance that every derived band below is computed against.
///
/// What IS removed is [`PACKAGED_SCRUB_ALWAYS`], plus
/// [`PACKAGED_SCRUB_GTK_RUNTIME`] and [`PACKAGED_SCRUB_FONT_STACK`] when
/// `gtk_runtime` is [`GtkRuntimeEnv::Scrubbed`]. `WOLF_FONT_PT` is NOT removed, and
/// that is not an oversight: it is set by [`cage_command`], not by the `shellHook`,
/// and it is the number [`cell_metrics_for_smoke`] measures against.
/// [`SMOKE_GTK_IM_MODULE`] is classified exactly the same way — harness isolation
/// set by [`cage_command`] rather than by the `shellHook` — so it is inherited here
/// and must never join [`PACKAGED_SCRUB_ALWAYS`].
///
/// `every_shellhook_export_is_scrubbed_or_deliberately_kept` is what keeps the three
/// scrub lists tracking `flake.nix` as it changes.
fn packaged_cage_command(
    scenario: &str,
    cwd: &Path,
    runtime: &CageRuntime<'_>,
    gtk_runtime: GtkRuntimeEnv,
) -> Command {
    let mut cmd = cage_command(scenario, cwd, runtime);
    for name in PACKAGED_SCRUB_ALWAYS {
        cmd.env_remove(name);
    }
    if matches!(gtk_runtime, GtkRuntimeEnv::Scrubbed) {
        for name in PACKAGED_SCRUB_GTK_RUNTIME {
            cmd.env_remove(name);
        }
        for name in PACKAGED_SCRUB_FONT_STACK {
            cmd.env_remove(name);
        }
    }
    cmd
}
