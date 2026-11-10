//! The packaged-artifact smokes, `#[ignore]`d: they drive `$out/bin/wolf-gnome` from
//! `nix build .#wolf-gnome` and run only through `./tools/wolf-package-smoke.sh`.
//!
//! They belong to this binary because they launch through `cage_command` and hold a
//! `CAGE_SLOTS` slot like every other smoke.

use super::editor::{OCR_WORD, SCENARIO};
use super::preview::{
    preview_theme_band_rows, preview_theme_pane_area, preview_theme_preview_columns,
    PREVIEW_DARK_BG, PREVIEW_OTHER_PANE_QUERY, PREVIEW_READBACK_PREFIX, PREVIEW_THEME_SOURCE_BODY,
    PREVIEW_THEME_SOURCE_FILE,
};
use super::*;

// --- Packaged-artifact smokes: here because they launch through `cage_command` like any other. ---
// See notes/crates/wolf-gnome/tests/smoke_headless/packaged.rs.md#packaged-smokes-share-this-binary

/// The environment variable carrying the store path under test.
///
/// ONE variable, with the binary derived from it, so the two cannot name different
/// outputs. `crates/wolf-gnome/tests/packaged_layout.rs` reads the same one.
const PACKAGE_OUT_ENV: &str = "WOLF_PACKAGE_OUT";

/// The command that produces the artifact.
const PACKAGE_BUILD_COMMAND: &str = "nix build .#wolf-gnome";

/// The one command that sets [`PACKAGE_OUT_ENV`] and runs every `#[ignore]`d test
/// that reads it. What makes an ignored test honest is that exactly one command runs
/// it and that command always establishes its precondition.
const PACKAGE_SMOKE_COMMAND: &str = "./tools/wolf-package-smoke.sh";

/// The installed executable, relative to the store root.
const PACKAGE_BIN_RELATIVE: &str = "bin/wolf-gnome";

/// The store path under test, checked to be a real package before any scenario runs.
///
/// Panics — never skips — on each of the four ways the precondition can be unmet.
/// That is the same rule [`require_smoke_env`] follows and for the same reason: a
/// skip would let these smokes report green on a machine that never built the
/// package, which is exactly the silently-skipped test `CLAUDE.md` forbids.
fn packaged_out(smoke: &str) -> PathBuf {
    let raw = match std::env::var(PACKAGE_OUT_ENV) {
        Ok(v) if !v.trim().is_empty() => v,
        Ok(_) => panic!(
            "{smoke}: {PACKAGE_OUT_ENV} is set but EMPTY. It must be the store path \
             `{PACKAGE_BUILD_COMMAND}` produced, and `{PACKAGE_SMOKE_COMMAND}` is what resolves it \
             and exports it. An empty value usually means the build step printed nothing, which is \
             a failed build reported as an empty string."
        ),
        Err(e) => panic!(
            "{smoke}: {PACKAGE_OUT_ENV} is not set ({e}). This smoke is `#[ignore]`d because it \
             drives the INSTALLED artifact rather than the checkout build, so it is run by exactly \
             one command: `{PACKAGE_SMOKE_COMMAND}`, which runs `{PACKAGE_BUILD_COMMAND}` and \
             exports the resulting store path. A PANIC and not a skip on purpose."
        ),
    };

    let out = PathBuf::from(raw);
    let meta = std::fs::metadata(&out).unwrap_or_else(|e| {
        panic!(
            "{smoke}: {PACKAGE_OUT_ENV} names {}, which cannot be read: {e}. A dangling `result` \
             symlink after a garbage collection looks exactly like this; re-run \
             `{PACKAGE_SMOKE_COMMAND}`.",
            out.display()
        )
    });
    assert!(
        meta.is_dir(),
        "{smoke}: {PACKAGE_OUT_ENV} names {}, which is not a directory. It must be the \
         derivation's `$out` ROOT, not the binary inside it — `{PACKAGE_BIN_RELATIVE}` is appended \
         to it here.",
        out.display()
    );
    out
}

/// The installed executable every packaged smoke launches, checked to exist.
fn packaged_binary(smoke: &str) -> PathBuf {
    let bin = packaged_out(smoke).join(PACKAGE_BIN_RELATIVE);
    assert!(
        bin.is_file(),
        "{smoke}: {} does not exist, so {PACKAGE_OUT_ENV} does not name a wolf-gnome package. \
         Re-run `{PACKAGE_SMOKE_COMMAND}`.",
        bin.display()
    );
    bin
}

// --- P1: the store binary launches and draws glyphs with none of the devShell's exports. ---

/// P1's capture, under `XDG_RUNTIME_DIR` like every other smoke's.
const PACKAGED_SMOKE_PNG_NAME: &str = "wolf_gnome_packaged_smoke.png";

/// The STORE binary launches under `cage` with none of the devShell's exports, and
/// renders typed text a human could read.
///
/// ## What it proves that the checkout smokes cannot
///
/// `window_renders_typed_text_under_cage` runs `CARGO_BIN_EXE_wolf-gnome` inside a
/// devShell that hands it `WOLF_NVIM_BIN`, `FONTCONFIG_FILE`, `XDG_DATA_DIRS`,
/// `GSETTINGS_SCHEMA_DIR` and the treesitter directories. This runs the SAME
/// scenario against `$out/bin/wolf-gnome` with every one of those removed, so a
/// single OCR hit says all of the following at once:
///
/// * the wrapper supplies `WOLF_NVIM_BIN`, so the app finds its Neovim;
/// * the app finds its captive `init.lua` through
///   [`wolf_gnome::init_lua::INSTALLED_INIT_LUA_RELATIVE`] rather than through the
///   `CARGO_MANIFEST_DIR` fallback, which names a build tree that does not exist in
///   the store;
/// * `wrapGAppsHook4` supplies the GSettings schemas, the icon theme and the mime
///   data that `adw::Application` aborts on `activate` without;
/// * the packaged, additive `FONTCONFIG_FILE` resolves a real face, since glyphs a
///   human could read is exactly what `tesseract` reports on.
///
/// ## The scrub is on the CHILD, not on this process
///
/// [`require_smoke_env`] still checks the TEST process's `WOLF_NVIM_BIN` and
/// `WOLF_RG_BIN`, because that is how it tells "the devShell was not entered" from
/// "the app failed". Those two variables are then removed from the `cage` command,
/// so the application under test never sees them.
///
/// ## Falsifiers
///
/// Delete `--set-default WOLF_NVIM_BIN` from `gappsWrapperArgs`: the app prints
/// `failed to start core`, opens no window, and the scenario's poll exits 11. Delete
/// branch 2 from `init_lua::resolve_init_lua`: Neovim gets a `-u` naming the build
/// tree, core never attaches, and the capture carries no typed word. Drop
/// `pkgs.gsettings-desktop-schemas` from `buildInputs`: `activate` aborts, again
/// exit 11.
///
/// ## What it needs
///
/// The full smoke toolchain, plus [`PACKAGE_OUT_ENV`]. Run through
/// `./tools/wolf-package-smoke.sh`.
#[test]
#[ignore = "drives the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn packaged_binary_renders_typed_text_under_cage() {
    const SMOKE: &str = "packaged_binary_renders_typed_text_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = packaged_binary(SMOKE);

    // The fillers, as `window_renders_typed_text_under_cage` has them: this smoke OCRs a whole page.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let out_png = runtime_dir.join(PACKAGED_SMOKE_PNG_NAME);
    clear_evidence(&out_png, SMOKE);

    let status = packaged_cage_command(SCENARIO, &cwd, &runtime_dir, GtkRuntimeEnv::Scrubbed)
        .env("WOLF_BIN", &bin)
        .env("OUT_PNG", &out_png)
        .env("OCR_WORD", OCR_WORD)
        .status()
        .expect("failed to spawn `cage` for the packaged headless smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}) for the PACKAGED binary at \
         {}. Exit 11 means the app died before the scenario could type — and with every devShell \
         export removed, that is the interesting failure rather than a mystery: the wrapper is not \
         carrying `WOLF_NVIM_BIN` (the app prints `failed to start core` and opens no window), or \
         `wrapGAppsHook4` is not carrying the GSettings schemas and the icon theme, without which \
         `adw::Application` aborts on `activate`. Run the same store binary by hand inside \
         `env -u WOLF_NVIM_BIN -u XDG_DATA_DIRS -u GSETTINGS_SCHEMA_DIR` to see the message. Any \
         other status is `wtype` or `grim` failing under `set -eu`. Capture expected at {}.",
        bin.display(),
        out_png.display()
    );

    // --- Assertion 2 (precondition): the capture exists -----------------------
    assert_png_present(SMOKE, &out_png);

    // --- Assertion 3 (discriminator): the packaged app drew readable glyphs ---
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(OCR_WORD),
        "{SMOKE}: OCR of the PACKAGED window did not contain the typed word {OCR_WORD:?}. The \
         scenario is byte-identical to `window_renders_typed_text_under_cage`'s, so if that one is \
         GREEN in the same run the difference is the artifact and the environment, not the \
         scenario — read its four documented signatures (a)-(d) first, then these three, which \
         only this smoke can produce. (1) The window drew but the editor grid is EMPTY of the \
         typed word and no tildes are visible: Neovim never attached, so look at the wrapper's \
         `WOLF_NVIM_BIN` and at the captive init.lua's installed path, which \
         `packaged_layout.rs::the_packaged_output_carries_the_captive_init_lua_at_the_resolvers_\
         own_path` checks independently. (2) The page drew but every glyph is a box or the wrong \
         shape: the packaged `FONTCONFIG_FILE` resolved no usable face, which is the one \
         mechanism in this package that was specified before it was executed. (3) Nothing drew at \
         all, yet the process survived: a GTK runtime the wrapper did not carry. PNG kept at {}. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

// --- P2: the packaged Neovim loads the vendored trees, against a checkout-binary control. ---

/// The Markdown fixture both launches open: a fenced block with short lines, and
/// nothing else.
///
/// Nothing in this smoke is OCR'd — its observables are six lines the captive Neovim
/// writes about itself and two pixel bands — so this name carries none of the
/// [`SIDEBAR_FILLER_FILES`] hazard and contributes no row to
/// `RELAUNCH_FIXTURE_NAMES`. That mirrors [`PREVIEW_THEME_SOURCE_FILE`] and
/// `CODEHL_SOURCE_FILE`, both absent from that table for the same stated reason: a
/// sidebar row is TEXT, and text can satisfy no assertion whose subject is a colour
/// or a `writefile` line.
const PACKAGED_PLUGINS_SOURCE_FILE: &str = "pkgfx.md";

/// The whole body of [`PACKAGED_PLUGINS_SOURCE_FILE`]: a five-line fenced block whose
/// longest content line is seven characters.
///
/// ## Why a fence, and why the lines are short
///
/// orlando's `code_block` feature fills a fenced block's rows to the grid edge —
/// fences inclusive, `hl_eol` fill — which
/// `crates/wolf-ffi/tests/orlando_bg_stress.rs`'s own doc comment states, so the
/// mechanism is established rather than hoped. That fill is what
/// [`packaged_code_band`] samples, in the RIGHT margin past the longest code line: a
/// region that is the fill colour when orlando is loaded and the ordinary editor
/// background when it is not.
///
/// ## Why it must stay FIVE lines
///
/// [`packaged_code_band`] and [`packaged_prose_band`] are derived from
/// [`PACKAGED_CODE_BLOCK_ROWS`] and [`PACKAGED_PROSE_BAND_FIRST_ROW`], so growing
/// this fixture pushes the fenced block into the prose band and the prose band off
/// the bottom of the grid. If more code rows are ever needed, move the two constants
/// with the fixture and re-derive both bands by hand.
const PACKAGED_PLUGINS_SOURCE_BODY: &str = "```txt\nalfa\nbravo\ncharlie\n```";

/// How many grid rows the fixture's fenced block occupies, fences included.
const PACKAGED_CODE_BLOCK_ROWS: f64 = 5.0;

/// How far inside the block's first and last rows [`packaged_code_band`] starts and
/// ends, in grid rows.
///
/// Half a row at each end, so the band tolerates the `PANE_ORIGIN_TOLERANCE_PX`-sized
/// uncertainty in [`PANE_ORIGIN_Y_EXPECTED`] without leaving the fill. Every row it
/// can slip onto is another row of the same block, because orlando fills the fences
/// too.
const PACKAGED_CODE_BAND_ROW_INSET: f64 = 0.5;

/// The first grid COLUMN of [`packaged_code_band`], counted from the text grid's own
/// left edge.
///
/// Past the fixture's longest code line (seven characters) by nine columns, so the
/// band is the block's right MARGIN — the region orlando's `hl_eol` fill reaches and
/// no glyph does.
const PACKAGED_CODE_BAND_FIRST_COL: f64 = 16.0;

/// The first grid ROW of [`packaged_prose_band`].
///
/// One row below the block, which leaves the row the parked cursor lands on (grid row
/// 5, after the scenario's `G` then `o` then Escape) outside BOTH bands.
const PACKAGED_PROSE_BAND_FIRST_ROW: f64 = 6.0;

/// The least of a sampled band that must be one single colour for the colour read at
/// its own corner to stand for the whole band.
///
/// ACCEPTANCE CRITERION, not a tuned number, and the same shape
/// `file_picker_covers_the_live_preview_under_cage` uses when it reads a widget's
/// fill out of a capture rather than naming it. Not 1.0, because the band's edges can
/// clip a glyph's antialiasing if the grid geometry has drifted by a pixel; far above
/// what a band straddling two surfaces could reach, which is what makes the reading
/// mean "this surface's colour" rather than "the colour at one corner".
const PACKAGED_BAND_MIN_UNIFORM_FRACTION: f64 = 0.98;

/// The most of the prose band that may be lighthouse's `Normal` background in the
/// CONTROL launch.
///
/// The half that makes the lighthouse band a discriminator rather than a presence
/// check: a launch with no colorscheme paints a different background entirely, so a
/// non-trivial fraction here would mean the two are indistinguishable and the smoke
/// could not tell a loaded colorscheme from an unloaded one.
const PACKAGED_CONTROL_MAX_LIGHTHOUSE_FRACTION: f64 = 0.01;

/// The `Normal` background the vendored lighthouse colorscheme sets in its DARK
/// variant, at the commit `flake.nix`'s `wolfVendoredPins` pins.
///
/// RE-TYPED here as a literal, never imported and never read out of the capture: the
/// claim is that the packaged app's captive Neovim is running THIS colorscheme, and
/// reading the value out of the thing under test would make the assertion a
/// comparison with itself. [`PREVIEW_DARK_BG`] sets the same precedent.
///
/// ONE constant behind TWO independent observables, deliberately: the captive
/// Neovim's own `nvim_get_hl(0, {name = 'Normal'}).bg` readback and the pixels the
/// front-end painted. A single edit re-pins both, and a run in which the readback
/// agrees and the pixels do not is a front-end that is not painting what Neovim
/// reports.
///
/// If a lighthouse bump moves it, RE-MEASURE and re-pin here rather than deleting the
/// assertion — the readback's failure message prints the value it actually saw.
const PACKAGED_LIGHTHOUSE_NORMAL_BG: Rgb8 = (0x1b, 0x1b, 0x1b);

/// The colour orlando's `code_block` feature fills a fenced block's rows with, at the
/// two commits `flake.nix`'s `wolfVendoredPins` pins.
///
/// MEASURED off launch B's retained capture, as the dominant colour of
/// [`packaged_code_band`]: `#252525` over 0.9443 of the band, against `#14161b` over
/// 1.0000 of the same rectangle in launch A.
///
/// It is the composition of TWO pins — orlando names the highlight group, the
/// lighthouse palette gives it a value — so a bump to EITHER re-measures it. Re-pin it
/// from the capture the failure message names; do not delete the assertion, and do not
/// substitute a colour read out of the launch under test, which would make the
/// assertion a comparison with itself.
///
/// It must differ from [`PACKAGED_LIGHTHOUSE_NORMAL_BG`], and assertion 9 checks that
/// before it counts anything: if the fill and the editor's own background ever
/// coincided, "the code band carries the fill" would be satisfied by a launch in which
/// orlando drew nothing at all.
const PACKAGED_ORLANDO_CODE_FILL: Rgb8 = (0x25, 0x25, 0x25);

/// The least of [`packaged_code_band`] that must be [`PACKAGED_ORLANDO_CODE_FILL`] in
/// the PACKAGED launch.
///
/// ## Why this is not 0.98, and must never be "tightened" to it
///
/// The fill CANNOT be uniform, and that is physics rather than a weak assertion. The
/// front-end draws one background rectangle per CELL, and Cairo antialiases each
/// rectangle's edges, so the band carries a one-pixel seam every cell. MEASURED: the
/// seams sit 22-23 px apart, which is exactly the cell pitch —
/// [`PANE_ORIGIN_X_EXPECTED`] + 16 * `cell_w` = 640 gives `cell_w` = 22.477 at
/// [`SMOKE_FONT_PT`] — so a row every one of whose cells is filled tops out at about
/// 0.961, and the whole band at the 0.9443 actually measured (the remainder is the
/// seams themselves: `#232323` at 0.0176, `#1b1b1b` at 0.0170, `#242424` at 0.0105 and
/// `#222222` at 0.0105, every one of them an intermediate between the fill and the
/// background).
///
/// 0.961 is therefore a HARD CEILING that no correct run can exceed, and 0.98 is
/// unreachable: a floor above 0.961 is red on a perfectly good package. This value
/// sits below the measurement with room for a glyph's antialiasing at a band edge, and
/// an order of magnitude above what an unfilled band reads, which is zero.
///
/// The number is font-independent: it follows from the cell pitch. The 0.9443 reading
/// was taken while this smoke still scrubbed `FONTCONFIG_FILE`; both launches keep it
/// now (see `PACKAGED_SCRUB_FONT_STACK`), so both resolve the face the cell
/// derivation above assumes and the fraction can only move TOWARDS the 0.961 ceiling.
/// If a run reads it moving the other way, report the number — that is a finding about
/// the fill, not a reason to lower this floor.
const PACKAGED_ORLANDO_MIN_FILL_FRACTION: f64 = 0.90;

/// The most of [`packaged_code_band`] that may be [`PACKAGED_ORLANDO_CODE_FILL`] in
/// the CONTROL launch.
///
/// The half that makes the floor above a DISCRIMINATOR rather than a presence check: a
/// checkout binary with no orlando paints the fenced block exactly like the rows below
/// it, so the fill colour cannot appear in this rectangle at all. MEASURED: 0.0000, the
/// band being uniformly `#14161b`. The ceiling is not zero only because a glyph edge
/// could in principle antialias into the fill's exact value; anything approaching the
/// floor above means the control has acquired the plugin, which readback lines 2 and 3
/// would also have caught. Do NOT raise it.
const PACKAGED_CONTROL_MAX_ORLANDO_FRACTION: f64 = 0.01;

/// How many lines the typed `:lua vim.fn.writefile(…)` writes.
const PACKAGED_READBACK_LINES: usize = 6;

/// What each of those lines carries, in order, for the failure messages.
const PACKAGED_READBACK_FIELDS: [&str; PACKAGED_READBACK_LINES] = [
    "vim.g.colors_name == 'lighthouse'",
    "vim.g.loaded_orlando is truthy",
    "pcall(require, 'orlando') succeeded",
    "nvim_get_hl(0, {name='Normal'}).bg as %06x",
    "vim.v.progpath",
    "vim.o.background",
];

/// The marker the readback writes for a satisfied condition, matching
/// `crates/wolf-ffi/tests/orlando_load.rs` and `lighthouse_load.rs`.
const PACKAGED_MARK_OK: &str = "OK";

/// The marker for an unsatisfied one.
const PACKAGED_MARK_NO: &str = "NO";

/// The CONTROL launch's readback file.
const PACKAGED_CONTROL_READBACK_NAME: &str = "wolf_gnome_packaged_control.txt";

/// The PACKAGED launch's readback file.
const PACKAGED_STORE_READBACK_NAME: &str = "wolf_gnome_packaged_store.txt";

/// The CONTROL launch's capture.
const PACKAGED_CONTROL_PNG_NAME: &str = "wolf_gnome_packaged_control.png";

/// The PACKAGED launch's capture.
const PACKAGED_STORE_PNG_NAME: &str = "wolf_gnome_packaged_store.png";

/// Name suffix of the control launch's own state directory sibling.
const PACKAGED_CONTROL_SUFFIX: &str = "-packaged-control";

/// Name suffix of the packaged launch's own state directory sibling.
const PACKAGED_STORE_SUFFIX: &str = "-packaged-store";

/// How a failure names the control launch, matching the scenario's stderr labels.
const PACKAGED_CONTROL_LABEL: &str = "launch A (the checkout binary, no sibling trees)";

/// How a failure names the packaged launch.
const PACKAGED_STORE_LABEL: &str = "launch B (the store binary)";

/// The absolute output row of the top of grid row `row`, in pixels.
///
/// [`PANE_ORIGIN_Y_EXPECTED`] names the TEXT GRID's corner — which is what a band
/// counted in grid rows has to be built from — and the cell comes from
/// [`cell_metrics_for_smoke`], the same `CellMetrics::for_font` call the app makes.
/// `OVERLAY_ORIGIN_Y_EXPECTED` is the WIDGET's corner and must not be substituted:
/// the two differ by a statusbar strip and a padding.
fn packaged_grid_row_top(row: f64) -> f64 {
    PANE_ORIGIN_Y_EXPECTED + row * cell_metrics_for_smoke().cell_h
}

/// The rectangle inside the fenced block's RIGHT MARGIN that orlando's `code_block`
/// fill reaches and no glyph does.
///
/// Every term is derived: the rows from [`PANE_ORIGIN_Y_EXPECTED`] plus the measured
/// cell height, the first column from [`PANE_ORIGIN_X_EXPECTED`] plus the measured
/// cell width, the last from [`CAGE_OUTPUT_W_EXPECTED`] less [`BAND_X_INSET_PX`].
/// Nothing here is a pixel literal.
fn packaged_code_band() -> SampleRect {
    let cell = cell_metrics_for_smoke();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    SampleRect {
        x0: pixel_index(
            PANE_ORIGIN_X_EXPECTED + PACKAGED_CODE_BAND_FIRST_COL * cell.cell_w,
            "the code band's first column",
        ),
        x1: pixel_index(output_w - BAND_X_INSET_PX, "the code band's last column"),
        y0: pixel_index(
            packaged_grid_row_top(PACKAGED_CODE_BAND_ROW_INSET),
            "the code band's first row",
        ),
        y1: pixel_index(
            packaged_grid_row_top(PACKAGED_CODE_BLOCK_ROWS - PACKAGED_CODE_BAND_ROW_INSET),
            "the code band's last row",
        ),
    }
}

/// The rectangle BELOW the fenced block and below the parked cursor's row, which is
/// the editor's own `Normal` background and nothing else.
///
/// Its columns start [`BAND_X_INSET_PX`] inside the pane area rather than at the
/// grid's own left edge, which is what keeps the end-of-buffer `~` markers in grid
/// column 0 and the parked block cursor out of it.
fn packaged_prose_band() -> SampleRect {
    let (area_x, _, _, _) = preview_theme_pane_area();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    SampleRect {
        x0: pixel_index(area_x + BAND_X_INSET_PX, "the prose band's first column"),
        x1: pixel_index(output_w - BAND_X_INSET_PX, "the prose band's last column"),
        y0: pixel_index(
            packaged_grid_row_top(PACKAGED_PROSE_BAND_FIRST_ROW),
            "the prose band's first row",
        ),
        y1: pixel_index(
            output_h - BAND_BOTTOM_CLEARANCE_PX,
            "the prose band's last row",
        ),
    }
}

/// Reads one launch's six-line readback, or panics naming which line is missing.
fn read_packaged_readback(path: &Path, smoke: &str, label: &str) -> Vec<String> {
    let raw = read_readback(path, smoke);
    let lines: Vec<String> = raw.lines().map(|line| line.trim().to_string()).collect();
    assert!(
        lines.len() == PACKAGED_READBACK_LINES,
        "{smoke}: {label}'s readback {} holds {} line(s), expected {PACKAGED_READBACK_LINES} \
         ({PACKAGED_READBACK_FIELDS:?}). Raw contents were {raw:?}. A SHORT file means the typed \
         `:lua vim.fn.writefile(…)` did not execute as written — the usual cause is a stray \
         keystroke landing in the buffer instead of the command line, which leaves a partial \
         command and a Lua error. An EMPTY one means `writefile` ran with an empty list.",
        path.display(),
        lines.len()
    );
    lines
}

/// The colour a band is, read at its own top-left corner and proved uniform.
///
/// The pattern `file_picker_covers_the_live_preview_under_cage` establishes: a
/// colour this file must not NAME — orlando's fill and lighthouse's palette are
/// upstream values at a pinned commit — is read out of the capture, and the
/// uniformity assertion is what licenses one pixel to stand for the whole band.
/// Without it a band straddling two surfaces would hand back whichever colour
/// happened to sit in its corner, and the relational comparison below would be
/// between two arbitrary pixels.
fn packaged_band_colour(
    smoke: &str,
    label: &str,
    what: &str,
    capture: &Capture,
    rect: SampleRect,
    png: &Path,
) -> Rgb8 {
    assert!(
        rect.x0 < rect.x1
            && rect.y0 < rect.y1
            && rect.x1 <= capture.width
            && rect.y1 <= capture.height,
        "{smoke}: {label}'s {what} band {rect:?} is degenerate or falls outside the {}x{} capture. \
         Every term is derived from CAGE_OUTPUT_*_EXPECTED, OVERLAY_ORIGIN_*_EXPECTED, \
         PANE_ORIGIN_*_EXPECTED and the cell `cell_metrics_for_smoke` measures, so this means two \
         of those have crossed — most plausibly a change to SMOKE_FONT_PT, which moves every grid \
         row at once. Re-derive the bands; do not shrink the insets.",
        capture.width,
        capture.height
    );

    let colour = capture.pixel(rect.x0, rect.y0);
    let hits = capture.count_exactly(rect, colour);
    let fraction = band_fraction(hits, rect.pixels(), "the band's uniform-colour fraction");
    assert!(
        fraction >= PACKAGED_BAND_MIN_UNIFORM_FRACTION,
        "{smoke}: {label}'s {what} band {rect:?} is only {fraction:.4} the colour {colour:?} at \
         its own top-left corner, below the {PACKAGED_BAND_MIN_UNIFORM_FRACTION} floor — so no \
         single pixel of it can stand for the band and every comparison built on it would be \
         between two arbitrary pixels. This helper is for bands that are ONE SURFACE: the prose \
         band, which is the editor's own `Normal` background and nothing else. It is deliberately \
         NOT used on the code band, whose per-cell antialiased seams cap its uniformity near 0.96 \
         — see PACKAGED_ORLANDO_MIN_FILL_FRACTION. Two causes read differently in the retained \
         capture. The band STRADDLES something: a glyph run reaching further right than the \
         fixture's longest code line, the parked cursor landing on a sampled row, or the fenced \
         block sitting at different grid rows from the ones PACKAGED_CODE_BLOCK_ROWS names. Or the \
         GRID GEOMETRY MOVED — but note what that CANNOT be here: both launches keep the \
         devShell's pinned FONTCONFIG_FILE (see PACKAGED_SCRUB_FONT_STACK), so both resolve the \
         same faces as `cell_metrics_for_smoke` measured and the cell cannot differ BETWEEN the \
         two. A geometry move is therefore a change to SMOKE_FONT_PT or to the window layout, \
         affecting both launches alike. Open {} before changing any number.",
        png.display()
    );
    colour
}

/// The two-launch VENDORED-PLUGIN scenario, run as one `sh -c` string inside ONE
/// `cage`, on `APPEARANCE_SCENARIO`'s and `PREVIEW_THEME_SCENARIO`'s two-launch
/// shape.
///
/// The launch body is a shell FUNCTION invoked twice rather than two copies, for the
/// reason those two smokes give: the runs must differ in nothing but their paths, and
/// a leg duplicated by hand is a leg that can be edited on one side only.
///
/// ## The one thing that differs, and how the difference is delivered
///
/// Launch A is `CARGO_BIN_EXE_wolf-gnome` — the checkout binary, which has no sibling
/// `orlando/` or `lighthouse/` beside its captive `init.lua` at all. Launch B is
/// `$out/bin/wolf-gnome`. Nothing else about the two launches differs.
///
/// `WOLF_NVIM_BIN` is removed at the `Command` level by `packaged_cage_command` and
/// re-exported INSIDE this scenario for launch A only, out of a variable passed under
/// a different name (`CONTROL_NVIM_BIN`). That is what makes "the packaged launch
/// never saw it" a demonstrated property rather than a claim: launch B runs with the
/// variable genuinely unset, and if the wrapper does not supply it the app prints
/// `failed to start core` and the scenario exits 15.
///
/// ## Each launch gets its own `WOLF_STATE_DIR`, and shares the config directory
///
/// The state directories must differ, or launch A's persisted layout is RESTORED
/// into launch B and the two are no longer the same run at two settings — see
/// [`smoke_state_dir_at`]. The CONFIG directory is deliberately shared, because the
/// font family, size, line-height and `appearance` are the premise both pixel bands
/// are derived against and must be identical on both sides.
///
/// ## Exit codes
///
/// Four per launch, derived from a base so the function takes seven parameters
/// rather than ten (a tenth positional needs `${10}`, which is a trap this file does
/// not need). The BASE itself means the app died before it was ready for input; base plus
/// one, still alive after `:qa!`; base plus two, exited non-zero; base plus three,
/// the readback never appeared. The offsets are spelled in words on purpose: written
/// as `+ 1` at the start of a line they open a Markdown list, and
/// `clippy::doc_lazy_continuation` then errors on every line that follows.
///
/// Launch A's base is 11 and launch B's is 15 — so `11`-`14` and `15`-`18`, the scheme
/// `APPEARANCE_SCENARIO` already uses. Eight codes and not four, for that smoke's
/// stated reason: "the app died before it was ready for input" is a different investigation
/// depending on which binary it was.
///
/// A `grim` failure has no code of its own: it aborts under `set -eu` and the cage
/// status carries grim's own.
const PACKAGED_PLUGINS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
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
    echo "wolf-gnome packaged-plugins smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# $1 is the exit code for "still alive after :qa!", $2 for "exited non-zero", and
# $3 is the label.
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
        echo "wolf-gnome packaged-plugins smoke: $3 app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome packaged-plugins smoke: $3 app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

# ONE launch, end to end. The two runs call it with different binaries, different
# state directories and different exit-code bases, and are otherwise the same run
# -- which is the whole design of this smoke, so the body must never be duplicated.
#
# The seven parameters are saved into named variables FIRST: wait_ready and
# quit_app below rebind $1..$3 for their own calls.
run_launch() {
    app_bin=$1
    state=$2
    png=$3
    readback=$4
    nvim_bin=$5
    base=$6
    label=$7

    code_died=$base
    code_alive=$((base + 1))
    code_status=$((base + 2))
    code_file=$((base + 3))

    # The state directory is per launch, deliberately overriding what
    # `cage_command` set: launch A quits with a layout of its own, and launch B
    # must not restore it. The CONFIG directory is left shared on purpose.
    WOLF_STATE_DIR="$state"
    export WOLF_STATE_DIR

    # WOLF_NVIM_BIN is re-exported for the CONTROL launch only, out of a variable
    # carried under a different name. Launch B is passed an empty string and runs
    # with the variable genuinely unset, so its Neovim can only have come from the
    # wrapper.
    if [ -n "$nvim_bin" ]; then
        WOLF_NVIM_BIN="$nvim_bin"
        export WOLF_NVIM_BIN
    else
        unset WOLF_NVIM_BIN || true
    fi

    "$app_bin" &
    app_pid=$!
    wait_ready "$app_pid" "$code_died" "wolf-gnome packaged-plugins smoke $label"

    # --- Step 1: open the fenced fixture in the window's ONE pane ------------
    wtype -- ":e $SRC_FILE"
    wtype -k Return
    sleep "$SETTLE"

    # --- Step 2: park the block cursor off both sampled bands ---------------
    #
    # `G` moves to the last line, `o` opens an empty line below it and `Escape`
    # leaves the cursor at COLUMN 0 of that new row -- which is one row below the
    # fenced block and left of both bands' first column. The inverted block would
    # otherwise corrupt whatever cell it covers.
    wtype -- "Go"
    wtype -k Escape
    sleep "$SETTLE"

    # --- Step 3: the capture ------------------------------------------------
    grim "$png"

    # --- Step 4: NEOVIM's own answer about the two vendored trees -----------
    #
    # Taken AFTER the capture on purpose: this typed command line is drawn by the
    # cmdline strip. The same instrument `crates/wolf-ffi/tests/orlando_load.rs`
    # and `lighthouse_load.rs` use against a real Neovim, extended to six lines.
    wtype -- ":lua local g=vim.g; local n=vim.api.nvim_get_hl(0,{name='Normal'}); vim.fn.writefile({(g.colors_name=='lighthouse' and 'OK' or 'NO'),(g.loaded_orlando and 'OK' or 'NO'),(pcall(require,'orlando') and 'OK' or 'NO'),string.format('%06x',n.bg or 0),vim.v.progpath,vim.o.background},'$readback')"
    wtype -k Return
    wait_file "$readback" || exit "$code_file"

    quit_app "$code_alive" "$code_status" "$label"
}

run_launch "$CONTROL_BIN" "$CONTROL_STATE_DIR" "$CONTROL_PNG" "$CONTROL_READBACK" \
    "$CONTROL_NVIM_BIN" 11 "launch A (checkout binary)"

run_launch "$STORE_BIN" "$STORE_STATE_DIR" "$STORE_PNG" "$STORE_READBACK" \
    "" 15 "launch B (store binary)"

exit 0
"#;

/// The PACKAGED app's captive Neovim really loads the vendored `lighthouse`
/// colorscheme and `orlando` plugin — against a control launch that has neither.
///
/// ## Why the A/B, and why the A side is not redundant
///
/// "lighthouse is active" is a claim about a colorscheme, and a colorscheme's
/// observable is a colour. Without the control leg, every one of this smoke's
/// assertions could be satisfied by a Neovim with no colorscheme at all that happened
/// to paint a similar background — and by construction nothing in a single launch
/// could tell the two apart. The control is the SAME scenario, the SAME fixture and
/// the SAME captive Neovim binary, differing only in which binary drives it: the
/// checkout build, whose `init.lua` has no sibling `orlando/` or `lighthouse/`
/// directory. Its readbacks must read `NO` where launch B's read `OK`, and its bands
/// must read a different colour.
///
/// ## What each of the six readback lines is for
///
/// | # | line | claim |
/// | --- | --- | --- |
/// | 1 | `vim.g.colors_name == 'lighthouse'` | `init.lua` found the sibling tree AND ran `:colorscheme`. A colorscheme merely on `runtimepath` does nothing. |
/// | 2 | `vim.g.loaded_orlando` | `plugin/orlando.lua` was sourced by `vim.cmd.runtime`. |
/// | 3 | `pcall(require, 'orlando')` | `lua/orlando/` is on `runtimepath`, which is a different fact from line 2 and fails separately. |
/// | 4 | `Normal`'s background | the colorscheme's own palette, not just its name. |
/// | 5 | `vim.v.progpath` | which Neovim the app launched. |
/// | 6 | `vim.o.background` | the seeded `appearance` reached the captive Neovim, which is the premise both bands rest on. |
///
/// ## Line 5 is property (C) MEASURED rather than argued
///
/// Launch A's captive Neovim came from the devShell's `WOLF_NVIM_BIN`; launch B's
/// came from the packaged wrapper. `v:progpath` reports the RESOLVED executable in
/// both, so equality says the package ships exactly the Neovim the Linux suite tests
/// against.
///
/// Comparing against this test process's own `WOLF_NVIM_BIN` string would NOT work
/// and must not be substituted: `pkgs.neovim` is a wrapper, and `v:progpath` resolves
/// through `/proc/self/exe` to the unwrapped store path, so the two strings
/// legitimately differ. Two `v:progpath` readings are comparable; a `v:progpath` and
/// a `lib.getExe` are not.
///
/// ## Why this smoke KEEPS the GTK runtime and the font stack
///
/// The three names are `XDG_DATA_DIRS`, `GSETTINGS_SCHEMA_DIR` and `FONTCONFIG_FILE`.
///
/// Launch A is the unwrapped checkout binary, which cannot reach `activate` without
/// the first two — `adw::Application` aborts. So the wrapped-GTK-runtime claim is NOT
/// made here; [`packaged_binary_renders_typed_text_under_cage`] is where it is made,
/// with the full scrub and only wrapped launches.
///
/// `FONTCONFIG_FILE` is kept for a different reason, and it is the one that would look
/// like a gap to a later reader. This smoke's subject is whether the two vendored Lua
/// TREES load. The font stack is an independent variable underneath it: the cell
/// [`cell_metrics_for_smoke`] measures is what [`packaged_code_band`] and
/// [`packaged_prose_band`] are derived from, so a packaged launch resolving a
/// different face would move the pane origin and every rectangle below it under launch
/// B alone — the A/B would then differ in two things and no band could say which one
/// it saw. The packaged-fontconfig claim belongs to
/// [`packaged_binary_renders_typed_text_under_cage`], which scrubs the variable and
/// OCRs the result. See `PACKAGED_SCRUB_FONT_STACK`.
///
/// ## The two pixel bands
///
/// [`packaged_prose_band`] samples the editor's own `Normal` background below the
/// fixture, and its assertion is against [`PACKAGED_LIGHTHOUSE_NORMAL_BG`] — a
/// re-typed literal, present in launch B and essentially absent in launch A.
///
/// [`packaged_code_band`] samples the right margin of the fenced block, which
/// orlando's `code_block` feature fills to the grid edge. Its assertion is PRESENCE
/// AGAINST ABSENCE of [`PACKAGED_ORLANDO_CODE_FILL`]: a floor of it in launch B and a
/// ceiling in launch A, both directions required. It is not a uniformity claim and
/// cannot be made one — the front-end draws one background rectangle per cell, and the
/// antialiased seam between them caps the band near 0.961 whatever orlando does. That
/// derivation, with the measured seam spacing, is in
/// [`PACKAGED_ORLANDO_MIN_FILL_FRACTION`].
///
/// **The orlando band's discriminating power is MEASURED, not inferred**: `#252525`
/// over 0.9443 of the band in the packaged launch against `#14161b` over 1.0000 of the
/// same rectangle in the control. So the feature row rests on pixels as well as on
/// readback lines 2 and 3, and neither is ticked on what the `hl_eol` mechanism
/// implies.
///
/// ## Falsifiers
///
/// Delete the `cp -R ${wolfLighthouse}/. …` line from `postInstall`, together with the
/// `lighthouse` operand of the `mkdir -p` above it — that line names BOTH trees, so
/// removing the whole of it would change the orlando leg as well, which this recipe is
/// not about. TAKEN, against
/// `/nix/store/gv1aiacbhxyrpjljbwz8zyw6wygdplcc-wolf-gnome-0.1.0`, and what it falsifies
/// is readback line 1 and ONLY that:
///
/// ```text
/// the vendored-tree readbacks disagree with the A/B:
///   * launch B (the store binary): `vim.g.colors_name == 'lighthouse'` read "NO", want "OK"
/// ```
///
/// The lighthouse band was never reached. The readback comparison is assertion 5 and the
/// band is assertion 8, so a missing tree aborts this test before one pixel of the prose
/// band has been counted — the same ordering the orlando deletion below runs into. Only
/// launch B's line is listed, so the control leg was untouched by the edit. The recipe is
/// worth taking for what it does reach; it cannot reach the band, and must not be
/// recorded as if it could.
///
/// Dropping the directory WITH the copy is load-bearing, and that is measured too. An
/// intermediate attempt left the `mkdir -p` alone and so installed an EMPTY `lighthouse/`,
/// which satisfies the `fs_stat` gate in `runtime/nvim/init.lua` exactly as a populated
/// tree does. The `vim.cmd.colorscheme("lighthouse")` two lines further on then errored,
/// `init.lua` aborted mid-file, and the application died on `attempt to call global
/// 'wolf_setup_current_file_tracking' (a nil value)` — scenario exit 18, launch B's
/// readback never appearing, which fails assertion 1 rather than assertion 5. That is the
/// empty-directory case: a louder and quite different failure from the missing-tree one
/// this falsifier is for. The trap is specific to lighthouse, because the orlando block
/// reaches its tree through `vim.cmd.runtime`, which matches nothing and does not error.
///
/// The edit that would isolate the LIGHTHOUSE band is the analogue of the orlando `sed`
/// below: leave the tree installed and activatable and change what it PAINTS in the
/// installed copy, so `vim.g.colors_name` still reads `lighthouse`, assertion 5 stays
/// green, and the band's colour is the only thing that moves. NOT TAKEN. Until somebody
/// takes it the band is unfalsified, and what is measured about the colorscheme is
/// readback lines 1 and 4.
///
/// Deleting the `wolfOrlando` copy falsifies readback lines 2 and 3, and ONLY those.
/// The readback comparison is assertion 5 and the orlando band's floor is assertion 9,
/// so a missing tree aborts this test before one pixel of the code band has been
/// counted. The recipe is worth taking for what it does reach; it cannot reach the
/// band, and must not be recorded as if it could.
///
/// The edit that isolates the band leaves the tree installed and requireable and turns
/// its `code_block` feature off in the INSTALLED copy, so the plugin loads and draws
/// nothing. Appended to `postInstall` after the `chmod -R u+w`:
///
/// ```text
/// sed -i '0,/enabled = true/! s/      enabled = true,/      enabled = false,/' \
///   "$out/share/wolf/nvim/orlando/lua/orlando/config.lua"
/// ```
///
/// which leaves that file's `wrap_indent.enabled` true (line 18) and makes its
/// `code_block.enabled` false (line 43). TAKEN, and all six readbacks stayed green
/// through that run, so the failure is attributable to the band alone:
///
/// ```text
/// launch B (the store binary): only 0.0000 of the code band
/// SampleRect { x0: 640, x1: 1200, y0: 206, y1: 382 } is
/// PACKAGED_ORLANDO_CODE_FILL = (37, 37, 37), below the 0.9 floor;
/// the prose band beside it reads (27, 27, 27).
/// ```
///
/// Put a lighthouse tree next to the CHECKOUT's `runtime/nvim/init.lua` and confirm
/// launch A's assertions fire, then remove it — that is the check that the A/B is an
/// A/B.
///
/// For line 5, point the package wrapper's `WOLF_NVIM_BIN` default in `flake.nix` at a
/// Neovim build other than `wolfNeovim` — not `neovim-unwrapped`, which is the binary
/// `v:progpath` already resolves to — and confirm the equality fires; if that cannot be
/// done cheaply, record the assertion as unfalsified rather than claiming a red run
/// nobody took.
#[test]
#[ignore = "drives the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage() {
    const SMOKE: &str = "the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage";

    let runtime_dir = require_smoke_env();
    let store_bin = packaged_binary(SMOKE);
    let control_bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Given to the control launch under another name, so the packaged one never sees `WOLF_NVIM_BIN`.
    let control_nvim = std::env::var("WOLF_NVIM_BIN").unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: WOLF_NVIM_BIN is not readable ({e}) even though `require_smoke_env` just \
             checked it. The control launch cannot start without it."
        )
    });

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(PACKAGED_PLUGINS_SOURCE_FILE, PACKAGED_PLUGINS_SOURCE_BODY)],
    );

    let control_state = smoke_state_dir_at(&cwd, PACKAGED_CONTROL_SUFFIX);
    let store_state = smoke_state_dir_at(&cwd, PACKAGED_STORE_SUFFIX);
    assert_ne!(
        control_state,
        store_state,
        "{SMOKE}: both launches were pointed at the SAME state directory {}. Launch A quits with a \
         layout of its own, which launch B would then RESTORE — at which point the two launches \
         are no longer the same run against two binaries. The two suffixes \
         ({PACKAGED_CONTROL_SUFFIX:?} and {PACKAGED_STORE_SUFFIX:?}) must differ.",
        control_state.display()
    );

    let control_png = runtime_dir.join(PACKAGED_CONTROL_PNG_NAME);
    let store_png = runtime_dir.join(PACKAGED_STORE_PNG_NAME);
    let control_readback = runtime_dir.join(PACKAGED_CONTROL_READBACK_NAME);
    let store_readback = runtime_dir.join(PACKAGED_STORE_READBACK_NAME);
    for path in [&control_png, &store_png, &control_readback, &store_readback] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = packaged_cage_command(
        PACKAGED_PLUGINS_SCENARIO,
        &cwd,
        &runtime_dir,
        GtkRuntimeEnv::Inherited,
    );
    cmd.env("SRC_FILE", PACKAGED_PLUGINS_SOURCE_FILE)
        .env("CONTROL_BIN", control_bin)
        .env("CONTROL_NVIM_BIN", &control_nvim)
        .env("CONTROL_STATE_DIR", &control_state)
        .env("CONTROL_PNG", &control_png)
        .env("CONTROL_READBACK", &control_readback)
        .env("STORE_BIN", &store_bin)
        .env("STORE_STATE_DIR", &store_state)
        .env("STORE_PNG", &store_png)
        .env("STORE_READBACK", &store_readback);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the packaged vendored-plugin scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Every code names ONE cause \
         in ONE launch. 11: launch A (the checkout binary at {control_bin}) died before it was \
         ready for input — it runs with the devShell's XDG_DATA_DIRS and GSETTINGS_SCHEMA_DIR kept \
         deliberately, so this is about the binary rather than the environment. 12: launch A was \
         still alive after `:qa!`. 13: launch A exited NON-ZERO — it went away, but not cleanly, \
         which is a separate code because a panic in the GTK main loop makes a process vanish \
         exactly as a clean shutdown does. 14: launch A's readback never appeared at {}. 15: \
         launch B (the store binary at {}) died before it was ready for input, and this is the \
         interesting one — launch B runs with WOLF_NVIM_BIN genuinely UNSET, so the wrapper is the \
         only thing that can supply it, and `failed to start core` is what it looks like when it \
         does not. 16/17/18: the same three for launch B, whose readback is at {}. Any other \
         status is `wtype` or `grim` failing under `set -eu`.",
        control_readback.display(),
        store_bin.display(),
        store_readback.display()
    );

    // --- Assertion 2 (precondition): both captures decode at the pinned size --
    assert_png_present(SMOKE, &control_png);
    assert_png_present(SMOKE, &store_png);
    let control = decode_capture(&control_png, SMOKE, PACKAGED_CONTROL_LABEL);
    let store = decode_capture(&store_png, SMOKE, PACKAGED_STORE_LABEL);

    // --- Assertion 3 (precondition): both readbacks are six lines -------------
    let control_lines = read_packaged_readback(&control_readback, SMOKE, PACKAGED_CONTROL_LABEL);
    let store_lines = read_packaged_readback(&store_readback, SMOKE, PACKAGED_STORE_LABEL);

    // --- Assertion 4 (precondition): the appearance premise both bands rest on
    for (label, lines) in [
        (PACKAGED_CONTROL_LABEL, &control_lines),
        (PACKAGED_STORE_LABEL, &store_lines),
    ] {
        assert_eq!(
            lines[5], SMOKE_APPEARANCE,
            "{SMOKE}: {label}'s `{}` readback is {:?}, expected {SMOKE_APPEARANCE:?}. Both \
             launches read the SAME seeded `wolf-config.yaml` through the shared WOLF_CONFIG_DIR \
             `cage_command` sets, so a disagreement here means one of them did not read it at all \
             — and every colour assertion below is about a DARK grid, so neither band would mean \
             what it says. `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` \
             guards the seeded value itself.",
            PACKAGED_READBACK_FIELDS[5], lines[5]
        );
    }

    // Assertion 5 (discriminator): collected, so a run where two of three regressed reports both.
    let mut wrong: Vec<String> = Vec::new();
    for line in 0..3 {
        if store_lines[line] != PACKAGED_MARK_OK {
            wrong.push(format!(
                "{PACKAGED_STORE_LABEL}: `{}` read {:?}, want {PACKAGED_MARK_OK:?}",
                PACKAGED_READBACK_FIELDS[line], store_lines[line]
            ));
        }
        if control_lines[line] != PACKAGED_MARK_NO {
            wrong.push(format!(
                "{PACKAGED_CONTROL_LABEL}: `{}` read {:?}, want {PACKAGED_MARK_NO:?}",
                PACKAGED_READBACK_FIELDS[line], control_lines[line]
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{SMOKE}: the vendored-tree readbacks disagree with the A/B:{}\n\
         \n\
         A `{PACKAGED_MARK_NO:?}` ON LAUNCH B means the packaged app's captive Neovim did not find \
         the tree beside its own `init.lua`. `runtime/nvim/init.lua` self-locates both trees from \
         its OWN directory and gates each on an `fs_stat`, so the failure is silent by \
         construction — nothing logs, nothing errors, the feature is simply not there. Check the \
         `postInstall` copies, and read \
         `packaged_layout.rs::the_packaged_output_carries_the_two_vendored_lua_trees_as_siblings`, \
         which asserts the same layout with no compositor.\n\
         \n\
         An `{PACKAGED_MARK_OK:?}` ON LAUNCH A is worse, because it disarms the whole smoke: the \
         CHECKOUT binary is not supposed to have either tree, so this means something put one \
         beside `runtime/nvim/init.lua` in the working tree — at which point launch B's `OK` says \
         nothing about the package. Remove it. Nothing in this repository installs a tree there, \
         which is exactly why neither derivation is in `linuxShell.packages`.\n\
         \n\
         Line 2 and line 3 fail SEPARATELY and mean different things: line 2 is \
         `plugin/orlando.lua` having been sourced, line 3 is `lua/orlando/` being on \
         `runtimepath`. An `OK`/`NO` split between them is a partial install rather than a missing \
         tree.",
        wrong
            .iter()
            .map(|w| format!("\n  * {w}"))
            .collect::<Vec<_>>()
            .join("")
    );

    // --- Assertion 6 (discriminator): the colorscheme's own palette -----------
    let (r, g, b) = PACKAGED_LIGHTHOUSE_NORMAL_BG;
    let want_bg = format!("{r:02x}{g:02x}{b:02x}");
    assert_eq!(
        store_lines[3], want_bg,
        "{SMOKE}: {PACKAGED_STORE_LABEL}'s `{}` readback is {:?}, expected {want_bg:?} — \
         PACKAGED_LIGHTHOUSE_NORMAL_BG, the vendored colorscheme's dark `Normal` background at the \
         pinned commit. Assertion 5 is green, so `vim.g.colors_name` IS `lighthouse`; what this \
         says is that the palette behind that name is not the one this file pins. The likely cause \
         is a lighthouse pin bump in `flake.nix`'s `wolfVendoredPins` — in which case RE-MEASURE \
         the value and re-pin the constant, which also re-pins the pixel band derived from it. Do \
         not delete the assertion: the name alone is satisfied by a colorscheme that loaded and \
         then failed to set a single highlight.",
        PACKAGED_READBACK_FIELDS[3], store_lines[3]
    );
    assert_ne!(
        control_lines[3], store_lines[3],
        "{SMOKE}: both launches report the SAME `{}` ({:?}). The control launch has no \
         colorscheme, so if its `Normal` background coincides with lighthouse's the pixel band \
         below cannot discriminate either and this smoke proves nothing about the colour. Two \
         readings are possible: the two Neovims really do agree, in which case this fixture needs \
         a different observable — or launch A somehow HAS the colorscheme, which assertion 5 \
         would also have caught.",
        PACKAGED_READBACK_FIELDS[3], store_lines[3]
    );

    // --- Assertion 7 (discriminator): the packaged Neovim IS the tested one ---
    assert_eq!(
        store_lines[4], control_lines[4],
        "{SMOKE}: the two launches report DIFFERENT `{}`: {PACKAGED_STORE_LABEL} ran {:?} and \
         {PACKAGED_CONTROL_LABEL} ran {:?}. Launch A's Neovim came from the devShell's \
         WOLF_NVIM_BIN — the binary every test in this repository is run against — and launch B's \
         came from the packaged wrapper, so a difference means the artifact ships a Neovim the \
         Linux evidence was NOT taken against, and every claim `runtime/nvim/init.lua` makes per \
         version is about a different one. The named cause is the package wrapper's \
         `WOLF_NVIM_BIN` default and `linuxShell`'s export not both reading `wolfNeovim`. Do \
         NOT 'fix' this by comparing against this process's own WOLF_NVIM_BIN string: \
         `pkgs.neovim` is a wrapper and `v:progpath` resolves \
         through /proc/self/exe to the UNWRAPPED store path, so those two legitimately differ and \
         the comparison would be red on a correct package. Two `v:progpath` readings are \
         comparable; a `v:progpath` and a `lib.getExe` are not.",
        PACKAGED_READBACK_FIELDS[4], store_lines[4], control_lines[4]
    );

    // --- Assertions 8 and 9: the two pixel bands ------------------------------
    let prose = packaged_prose_band();
    let code = packaged_code_band();

    let control_prose = packaged_band_colour(
        SMOKE,
        PACKAGED_CONTROL_LABEL,
        "prose",
        &control,
        prose,
        &control_png,
    );
    let store_prose = packaged_band_colour(
        SMOKE,
        PACKAGED_STORE_LABEL,
        "prose",
        &store,
        prose,
        &store_png,
    );

    // The code band is counted, not read, so its non-degeneracy is checked here; a collapsed band would
    // report 0.0000 and pass assertion 9's ceiling on nothing.
    for (label, capture) in [
        (PACKAGED_CONTROL_LABEL, &control),
        (PACKAGED_STORE_LABEL, &store),
    ] {
        assert!(
            code.x0 < code.x1
                && code.y0 < code.y1
                && code.x1 <= capture.width
                && code.y1 <= capture.height,
            "{SMOKE}: {label}'s code band {code:?} is degenerate or falls outside the {}x{} \
             capture. Every term is derived from CAGE_OUTPUT_*_EXPECTED, PANE_ORIGIN_*_EXPECTED \
             and the cell `cell_metrics_for_smoke` measures, so this means two of those have \
             crossed — most plausibly a change to SMOKE_FONT_PT, which moves every grid row and \
             column at once. Re-derive the band; do not shrink PACKAGED_CODE_BAND_FIRST_COL.",
            capture.width,
            capture.height
        );
    }

    // --- Assertion 8: the LIGHTHOUSE band, both directions --------------------
    let store_hits = store.count_exactly(prose, PACKAGED_LIGHTHOUSE_NORMAL_BG);
    let store_fraction = band_fraction(
        store_hits,
        prose.pixels(),
        "the packaged launch's lighthouse fraction",
    );
    assert!(
        store_fraction >= PACKAGED_BAND_MIN_UNIFORM_FRACTION,
        "{SMOKE}: {PACKAGED_STORE_LABEL}: only {store_fraction:.4} of the prose band {prose:?} is \
         PACKAGED_LIGHTHOUSE_NORMAL_BG = {PACKAGED_LIGHTHOUSE_NORMAL_BG:?}; the band's own colour \
         reads {store_prose:?}. Readback line 4 already agreed with the constant, so Neovim \
         BELIEVES `Normal` is that colour and the front-end painted something else — which is a \
         claim about `render::paint` and the snapshot's default background rather than about the \
         package. If readback line 4 is ALSO red, fix that first: the two share one constant on \
         purpose. Capture kept at {}.",
        store_png.display()
    );
    let control_hits = control.count_exactly(prose, PACKAGED_LIGHTHOUSE_NORMAL_BG);
    let control_fraction = band_fraction(
        control_hits,
        prose.pixels(),
        "the control launch's lighthouse fraction",
    );
    assert!(
        control_fraction <= PACKAGED_CONTROL_MAX_LIGHTHOUSE_FRACTION,
        "{SMOKE}: {PACKAGED_CONTROL_LABEL}: {control_fraction:.4} of the prose band {prose:?} is \
         PACKAGED_LIGHTHOUSE_NORMAL_BG = {PACKAGED_LIGHTHOUSE_NORMAL_BG:?}, above the \
         {PACKAGED_CONTROL_MAX_LIGHTHOUSE_FRACTION} ceiling; the band's own colour reads \
         {control_prose:?}. This is the half that makes the assertion above a DISCRIMINATOR: a \
         control that paints the same background as lighthouse would satisfy the packaged \
         assertion whether or not the colorscheme ever loaded. A fraction near 1.0 means the \
         checkout binary somehow has the colorscheme — check for a stray `lighthouse/` beside \
         `runtime/nvim/init.lua`, which assertion 5 would also have caught. Do NOT raise this \
         ceiling. Capture kept at {}.",
        control_png.display()
    );

    // Assertion 9: presence against absence, not uniformity, since antialiased seams cap the band near
    // 0.961. See PACKAGED_ORLANDO_MIN_FILL_FRACTION.

    // Non-vacuity: if orlando's fill matched the editor background, the floor would pass with nothing
    // drawn and the ceiling on a mere colorscheme difference.
    assert_ne!(
        PACKAGED_ORLANDO_CODE_FILL, PACKAGED_LIGHTHOUSE_NORMAL_BG,
        "{SMOKE}: PACKAGED_ORLANDO_CODE_FILL and PACKAGED_LIGHTHOUSE_NORMAL_BG are the SAME \
         colour, so assertion 9 cannot tell orlando's `code_block` fill from the editor's own \
         `Normal` background and would pass on a package carrying no orlando at all. One of the \
         two constants was re-pinned against the wrong band; re-measure both off the retained \
         captures."
    );

    let store_fill_hits = store.count_exactly(code, PACKAGED_ORLANDO_CODE_FILL);
    let store_fill_fraction = band_fraction(
        store_fill_hits,
        code.pixels(),
        "the packaged launch's orlando-fill fraction",
    );
    assert!(
        store_fill_fraction >= PACKAGED_ORLANDO_MIN_FILL_FRACTION,
        "{SMOKE}: {PACKAGED_STORE_LABEL}: only {store_fill_fraction:.4} of the code band {code:?} \
         is PACKAGED_ORLANDO_CODE_FILL = {PACKAGED_ORLANDO_CODE_FILL:?}, below the \
         {PACKAGED_ORLANDO_MIN_FILL_FRACTION} floor; the prose band beside it reads \
         {store_prose:?}. Three readings are distinguishable and the retained capture separates \
         them. NEAR ZERO with the code band the same colour as the prose band: orlando's \
         `code_block` fill reached no pixel, which — since readback lines 2 and 3 say the plugin \
         IS loaded — is loaded-but-not-drawn rather than a missing tree. NEAR ZERO with the band \
         some THIRD colour: the band is aimed at something else entirely (the fixture's block \
         occupies grid rows 0..{PACKAGED_CODE_BLOCK_ROWS} and the band's first column is \
         {PACKAGED_CODE_BAND_FIRST_COL}), so re-derive it. BETWEEN the two, around 0.5: the band \
         straddles the block's edge. Do NOT lower this floor to accept a partial reading, and do \
         not raise it towards 0.98 either — 0.961 is a hard ceiling the per-cell seams impose. \
         Capture kept at {}.",
        store_png.display()
    );
    let control_fill_hits = control.count_exactly(code, PACKAGED_ORLANDO_CODE_FILL);
    let control_fill_fraction = band_fraction(
        control_fill_hits,
        code.pixels(),
        "the control launch's orlando-fill fraction",
    );
    assert!(
        control_fill_fraction <= PACKAGED_CONTROL_MAX_ORLANDO_FRACTION,
        "{SMOKE}: {PACKAGED_CONTROL_LABEL}: {control_fill_fraction:.4} of the code band {code:?} \
         is PACKAGED_ORLANDO_CODE_FILL = {PACKAGED_ORLANDO_CODE_FILL:?}, above the \
         {PACKAGED_CONTROL_MAX_ORLANDO_FRACTION} ceiling; the prose band beside it reads \
         {control_prose:?}. This is the half that makes the floor above a DISCRIMINATOR: the \
         checkout binary has no orlando, so its fenced block must be painted exactly like the rows \
         below it, and a fill-coloured code band here means something OTHER than the plugin fills \
         those rows — a treesitter `@markup.raw.block` highlight, a `CursorLine` that reached the \
         band, or a stray `orlando/` beside `runtime/nvim/init.lua`, which assertion 5 would also \
         have caught. At that point launch B's floor is satisfied by that something rather than by \
         the plugin. Move the band or the fixture; do not raise this ceiling. Capture kept at {}.",
        control_png.display()
    );
}

// --- P3: the packaged app opens a preview, catching WebKit helpers unresolved in the store. ---

/// P3's capture.
const PACKAGED_PREVIEW_PNG_NAME: &str = "wolf_gnome_packaged_preview.png";

/// P3's window-list readback.
const PACKAGED_PREVIEW_WINS_NAME: &str = "wolf_gnome_packaged_preview_wins.txt";

/// The least of [`packaged_preview_document_band`] that must be [`PREVIEW_DARK_BG`].
///
/// MEASURED on the packaged launch: 1.0000 of that rectangle. What the floor rules out
/// is a web view that is ALIVE and showing something which is not the rendered
/// document — `about:blank`, and the empty string `WolfCore::render_preview` answers on
/// an RPC failure, both paint WHITE, so either reads as approximately zero here rather
/// than as a near miss. It does NOT rule out a pane painted with the editor's own
/// background, because for this binary that background is the same colour; that is
/// what [`PACKAGED_PREVIEW_MIN_DOC_INK_PX`] is for, and neither half stands alone.
const PACKAGED_PREVIEW_MIN_DOC_BG_FRACTION: f64 = 0.90;

/// The fewest non-[`PREVIEW_DARK_BG`] pixels [`packaged_preview_ink_band`] must carry.
///
/// The other half of "the right pane renders the document": a pane that is dark but
/// EMPTY reads zero here. That is exactly what an editor grid over the preview scratch
/// buffer would look like — the scratch holds no text, and under the vendored
/// colorscheme its background is [`PACKAGED_LIGHTHOUSE_NORMAL_BG`], the same
/// `#1b1b1b` — so without this count the background floor above would be satisfied by
/// a launch whose web view never rendered anything at all.
///
/// ACCEPTANCE CRITERION rather than a tuned number, and the same one
/// `MIN_FLUSH_INK_PX` uses, for the same reason: it must sit far above stray
/// antialiasing at a band edge and far below what real body text draws. MEASURED by
/// `preview_document_follows_the_configured_appearance_under_cage` across these same
/// 354 columns: 1,987 non-theme pixels in ONE 24-row line box. The band here is 200
/// rows tall — room for some eight such boxes, of which [`PREVIEW_THEME_SOURCE_BODY`]'s
/// wrapped paragraph fills about three — so the floor sits an order of magnitude below
/// what a SINGLE rendered line contributes.
const PACKAGED_PREVIEW_MIN_DOC_INK_PX: usize = 200;

/// The rectangle inside the PREVIEW (right) pane that the rendered document's own
/// background fills.
///
/// `preview_document_follows_the_configured_appearance_under_cage`'s sample band
/// verbatim, through the same two helpers rather than re-derived: it is calibrated to
/// sit below everything [`PREVIEW_THEME_SOURCE_BODY`] renders, and this smoke opens
/// that same fixture.
fn packaged_preview_document_band() -> SampleRect {
    let (x0, x1) = preview_theme_preview_columns();
    let (y0, y1) = preview_theme_band_rows();
    SampleRect { x0, x1, y0, y1 }
}

/// The rectangle inside the preview pane where the rendered document's TEXT is: the
/// same columns, from the pane's text-grid line down to the background band.
///
/// ## The premise it rests on, stated rather than assumed
///
/// Its first row is [`PANE_ORIGIN_Y_EXPECTED`] — the EDITOR pane's text-grid corner —
/// used here as a lower bound on where the preview pane's own chrome ends. Both panes
/// carry the same statusbar strip and padding above their content, and
/// `preview_document_follows_the_configured_appearance_under_cage` MEASURED the web
/// view's own top edge at absolute row 159 against this constant's 184, so the band
/// starts 25 rows inside the view. That clearance is what keeps the statusbar's own
/// glyphs — which are ink, and would satisfy the ink floor by themselves — out of the
/// rectangle.
///
/// The band is deliberately NOT self-located from the pixels the way that smoke's
/// `web_top` scan is. That scan finds the first row entirely the theme colour, which
/// works only where the editor's background differs from the document's; for the
/// PACKAGED binary they are the same `#1b1b1b`, so the scan would stop on a row of
/// pane padding above the view and the band would slide up into the chrome. See this
/// smoke's doc comment.
fn packaged_preview_ink_band() -> SampleRect {
    let (x0, x1) = preview_theme_preview_columns();
    let (band_y0, _) = preview_theme_band_rows();
    SampleRect {
        x0,
        x1,
        y0: pixel_index(PANE_ORIGIN_Y_EXPECTED, "the preview ink band's first row"),
        y1: band_y0,
    }
}

/// The single-launch PACKAGED PREVIEW scenario: `PREVIEW_THEME_SCENARIO`'s
/// `run_launch` body with one launch and no per-launch config directory, since the
/// appearance is not the variable here.
///
/// Exit codes: `11` the app died before it was ready for input, `12` still alive after
/// `:qa!`, `13` exited non-zero, `14` the window-list readback never appeared.
const PACKAGED_PREVIEW_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event.
# PREVIEW_SCENARIO's doc comment carries the measurement behind it. This is a cold
# open: a brand new process with a brand new WebProcess. Do not trim it.
PREVIEW_SETTLE=2.0

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
wait_ready "$app_pid" 11 "wolf-gnome packaged-preview smoke"

# --- Step 1: open the SOURCE in the window's ONE pane ------------------------
#
# One pane is what makes the geometry deterministic: the other-pane Preview below
# splits it, and a split of ONE pane can only produce two.
wtype -- ":e $SRC_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: drive PREVIEW IN ANOTHER PANE through the command palette --------
#
# The OTHER-PANE command and not plain Preview: Preview binds in place, so from this
# window's one pane it would leave no second pane and the bands below would have no
# right half to sample.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_OTHER_PANE_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 3: the capture, with the preview pane live -------------------------
grim "$OUT_PNG"

# --- Step 4: NEOVIM's own answer about what its windows hold -----------------
#
# Taken AFTER the capture: this typed command line is drawn by the cmdline strip.
# Joined with a COMMA rather than a space because a preview scratch's own name
# contains one.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS')"
wtype -k Return

t=0
while [ "$t" -lt "$FILE_TRIES" ]; do
    if [ -s "$WINS" ]; then
        break
    fi
    t=$((t + 1))
    sleep "$WAIT_INTERVAL"
done
if [ ! -s "$WINS" ]; then
    echo "wolf-gnome packaged-preview smoke: readback $WINS never appeared." >&2
    exit 14
fi

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
    echo "wolf-gnome packaged-preview smoke: app (pid $app_pid) still alive after :qa!." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome packaged-preview smoke: app (pid $app_pid) exited $rc, not 0." >&2
    exit 13
fi
app_pid=

exit 0
"#;

/// The PACKAGED app opens a Markdown preview and renders the document in it, with
/// none of the devShell's exports.
///
/// ## Why this smoke exists, and what it settled
///
/// A `WebKitWebView` spawns out-of-process `WebKitNetworkProcess` and
/// `WebKitWebProcess` helpers. Whether they are found through `PATH` matters, because
/// the wrapper reproduces no `PATH`. If they needed it,
/// the packaged preview pane would be blank or the app would die, and
/// NOTHING else in this repository would have noticed: every other packaged claim is
/// about the editor grid, which WebKit is not involved in.
///
/// It was an EXPERIMENT with a decision rule, and the experiment is finished. Run with
/// no `WEBKIT_EXEC_PATH`, it passes. MEASURED, independently of the run: the helper
/// path is baked into `libwebkitgtk-6.0.so.4` as an absolute store path, and the token
/// `WEBKIT_EXEC_PATH` does not appear in the library at all. So the variable is NOT
/// set by `gappsWrapperArgs` — setting one nothing verified is the thing to avoid in
/// either direction. The finding is recorded in notes/flake.nix.md#webkitgtk-and-libsoup.
/// This smoke stays as the thing that would notice if a WebKitGTK bump reintroduced
/// the dependency.
///
/// ## The appearance discriminator is NOT available here, and that is a measurement
///
/// The sibling smoke's `assert_preview_theme_launch` rests on the editor pane's
/// background differing from the preview theme's: assertion 4 reads the EDITOR band as
/// a named control, and assertion 6's `web_top` scan is only meaningful because the
/// chrome above the web view is a different colour from the document behind it.
///
/// For the PACKAGED binary those two colours COINCIDE BY CONSTRUCTION. The vendored
/// colorscheme's `Normal` background is [`PACKAGED_LIGHTHOUSE_NORMAL_BG`] and the dark
/// preview theme's is [`PREVIEW_DARK_BG`], and both are `#1b1b1b`. MEASURED in this
/// smoke's own capture: the editor pane's band reads 0.8177 of that colour and the
/// preview pane's 1.0000, so the control band cannot discriminate anything and the
/// `web_top` scan would stop on a row of pane padding above the view rather than on
/// the view's own top edge.
///
/// So this smoke does not reuse those three assertions, and the light/dark claim is
/// NOT made here. It belongs to
/// `preview_document_follows_the_configured_appearance_under_cage`, which makes it
/// against the CHECKOUT binary — whose editor background is `#14161b` — with a two
/// launch pair that packaging does not change.
///
/// ## What this smoke does claim
///
/// Three things, and the doc comment says so plainly rather than leaving a reader to
/// infer it from the code: the scenario completes (assertion 1), the captive Neovim's
/// windows hold both the source buffer and the preview scratch (assertion 3), and the
/// RIGHT pane renders the document (assertions 4 and 5). The last is asserted from two
/// sides that fail on different things — a floor of the document's own background over
/// [`packaged_preview_document_band`], which a white `about:blank` reads as zero, and a
/// floor of INK over [`packaged_preview_ink_band`], which a dark-but-empty pane reads
/// as zero. Neither half stands alone.
///
/// MEASURED for assertion 5 on this smoke's own capture, as the ink band's five
/// commonest colours: `#1b1b1b` 66,781 pixels, `#d4d4d4` 481, `#1e1e1e` 135,
/// `#4c4c4c` 102, `#1c1c1c` 88, over a 70,800-pixel band. That leaves 4,019 pixels
/// which are not the document's background, against the floor of 200. The SHAPE of
/// that tail settles more than the count does: a long spread of shades between the
/// background and the body colour is what text antialiasing draws, and not what a
/// chrome edge draws — an edge shows two flat colours and a thin seam. So the band is
/// measured to hold GLYPHS, which is the reading that confirms, for the PACKAGED
/// binary, the 25-row clearance [`packaged_preview_ink_band`] infers from the checkout
/// binary's measured web-view top edge. Without that reading the clearance rests on an
/// inference carried from one build to another.
///
/// One launch and not two: the appearance pair is the sibling smoke's claim, and what
/// is new here is only that the packaged app renders a document at all. The launch runs
/// at the shared seeded [`SMOKE_APPEARANCE`], which is `dark`, so [`PREVIEW_DARK_BG`]
/// is the document's background.
///
/// ## Falsifiers
///
/// Add `--set-default WEBKIT_EXEC_PATH /nonexistent` to `gappsWrapperArgs`: WebKit's
/// helpers stop resolving, and a helper that cannot be spawned takes the process with
/// it rather than degrading, so assertion 1 fires with exit 13. Delete the
/// `OpenPreview` arm from `shell::perform`: assertion 3 fires. Make
/// `WolfCore::render_preview` answer the empty string: the view paints white,
/// assertion 4 fires.
#[test]
#[ignore = "drives the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_binary_opens_a_preview_under_cage() {
    const SMOKE: &str = "the_packaged_binary_opens_a_preview_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = packaged_binary(SMOKE);

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(PREVIEW_THEME_SOURCE_FILE, PREVIEW_THEME_SOURCE_BODY)],
    );

    let out_png = runtime_dir.join(PACKAGED_PREVIEW_PNG_NAME);
    let wins = runtime_dir.join(PACKAGED_PREVIEW_WINS_NAME);
    for path in [&out_png, &wins] {
        clear_evidence(path, SMOKE);
    }

    let status = packaged_cage_command(
        PACKAGED_PREVIEW_SCENARIO,
        &cwd,
        &runtime_dir,
        GtkRuntimeEnv::Scrubbed,
    )
    .env("WOLF_BIN", &bin)
    .env("SRC_FILE", PREVIEW_THEME_SOURCE_FILE)
    .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
    .env("OUT_PNG", &out_png)
    .env("WINS", &wins)
    .status()
    .expect("failed to spawn `cage` for the packaged preview scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}) for the PACKAGED binary at \
         {}. Exit 11: the app died before it was ready for input. Exit 12: still alive after `:qa!`. \
         Exit 13: exited NON-ZERO, which is the one to read closely here — WebKit's mandatory \
         bubblewrap sandbox and its out-of-process helpers are what this smoke exists to \
         exercise, and a helper that cannot be spawned takes the process with it rather than \
         degrading. Exit 14: the window-list readback never appeared at {}. Any other status is \
         `wtype` or `grim` failing under `set -eu`. If this is where the experiment lands, the \
         decision rule is in this test's doc comment: add `--set-default WEBKIT_EXEC_PATH` to \
         `gappsWrapperArgs` and re-run.",
        bin.display(),
        wins.display()
    );

    // --- Assertion 2 (precondition): the capture decodes at the pinned size ---
    assert_png_present(SMOKE, &out_png);
    let capture = decode_capture(&out_png, SMOKE, "the packaged preview launch");

    // --- Assertion 3 (precondition): a preview pane exists at all -------------
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{PREVIEW_THEME_SOURCE_FILE}"),
            PREVIEW_THEME_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got = read_window_buffer_names(&wins, SMOKE);
    assert_eq!(
        got, want_windows,
        "{SMOKE}: after the packaged launch's Preview leg the captive Neovim's windows hold \
         {got:?}, not {want_windows:?} (both sorted, so window order is not the subject). A SINGLE \
         entry naming {PREVIEW_THEME_SOURCE_FILE:?} means no preview was bound at all — in a \
         PACKAGED run the first thing to check is the palette itself, since the command registry \
         and the Ctrl+Shift+P chord are unaffected by packaging, and the second is whether the web \
         view could be constructed. A SINGLE entry naming the preview scratch means the IN-PLACE \
         Preview ran rather than the other-pane one. Every rectangle below assumes exactly two \
         side-by-side panes."
    );

    // Assertions 4 and 5: not `assert_preview_theme_launch`, whose control band needs the editor and
    // preview backgrounds to differ; here both are `#1b1b1b`. See the doc comment above.
    let document = packaged_preview_document_band();
    let ink = packaged_preview_ink_band();
    for (what, rect) in [("document", document), ("ink", ink)] {
        assert!(
            rect.x0 < rect.x1
                && rect.y0 < rect.y1
                && rect.x1 <= capture.width
                && rect.y1 <= capture.height,
            "{SMOKE}: the {what} band {rect:?} is degenerate or falls outside the {}x{} capture. \
             Both are derived from OVERLAY_ORIGIN_*_EXPECTED, PANE_ORIGIN_Y_EXPECTED, \
             BAND_X_INSET_PX and BAND_BOTTOM_CLEARANCE_PX, so this means two of those have \
             crossed. Re-derive them; do not shrink the insets, which are what keep the bands \
             clear of the `gtk::Paned` handle and the command-line strip.",
            capture.width,
            capture.height
        );
    }

    // --- Assertion 4: the document's own background fills the pane ------------
    let bg_hits = capture.count_exactly(document, PREVIEW_DARK_BG);
    let bg_fraction = band_fraction(
        bg_hits,
        document.pixels(),
        "the packaged preview's document-background fraction",
    );
    assert!(
        bg_fraction >= PACKAGED_PREVIEW_MIN_DOC_BG_FRACTION,
        "{SMOKE}: only {bg_fraction:.4} of the document band {document:?} — inside the RIGHT, \
         preview pane — is PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?}, below the \
         {PACKAGED_PREVIEW_MIN_DOC_BG_FRACTION} floor. A reading near ZERO means the web view is \
         alive and showing something that is not the rendered document: `about:blank`, or the \
         empty string `WolfCore::render_preview` answers on an RPC failure, both of which paint \
         WHITE. A reading in between means the band is straddling the pane boundary, which is a \
         geometry finding rather than a packaging one — the two panes are equal halves of the pane \
         area, so check `PaneTree::split`'s weights before touching this number. Note what this \
         assertion deliberately does NOT say: the editor's own background is the same `#1b1b1b` \
         for this binary, so a dark band alone would not prove a document is there at all. \
         Assertion 5 is the half that does. Capture kept at {}.",
        out_png.display()
    );

    // --- Assertion 5: and it carries the document's INK ------------------------
    let ink_pixels = ink.pixels() - capture.count_exactly(ink, PREVIEW_DARK_BG);
    assert!(
        ink_pixels >= PACKAGED_PREVIEW_MIN_DOC_INK_PX,
        "{SMOKE}: the ink band {ink:?} — the same preview columns, from the pane's text-grid line \
         down to the document band — carries only {ink_pixels} non-PREVIEW_DARK_BG pixels, fewer \
         than the {PACKAGED_PREVIEW_MIN_DOC_INK_PX} floor. Assertion 4 is green, so the pane IS \
         `#1b1b1b`; this says nothing is drawn ON it. The failure this pairing exists to catch is \
         a pane that is dark and EMPTY — an editor grid over the empty preview scratch buffer, \
         whose background under the vendored colorscheme is exactly the same colour, so assertion \
         4 alone would accept it. Two other readings are possible from the retained capture. If \
         the document is visibly rendered but sits lower than this band, the pane chrome grew and \
         the band's premise (its first row is 25 rows inside the web view, measured) no longer \
         holds — re-measure, do not lower the floor. If every glyph is a box, the packaged \
         FONTCONFIG_FILE resolved no usable face, which \
         `packaged_binary_renders_typed_text_under_cage` would also have caught in the editor. \
         Capture kept at {}.",
        out_png.display()
    );
}
