//! The terminal-colours smoke: under the vendored Lighthouse, a terminal pane paints the 16 ANSI
//! colours as Lighthouse's, for the dark appearance and for the light one.

use super::lighthouse::{rgb8, Background, Lighthouse};
use super::*;

/// The `SHELL` a typed `:terminal` runs: `X` in SGR 30–37 and 90–97 on row 0, a space in SGR
/// 40–47 and 100–107 on row 1, each followed by `ESC[0m`, so ANSI colour `i` is column `i`.
/// No cell is bold, so no brightening applies. Once both rows are printed it writes
/// `$TC_PAINTED` and sleeps, so the session stays open until the app goes.
fn colour_shell() -> String {
    use std::fmt::Write as _;

    let mut fg = String::new();
    let mut bg = String::new();
    for i in 0..16 {
        let _ = write!(fg, "\\033[{}mX\\033[0m", sgr_fg(i));
        let _ = write!(bg, "\\033[{}m \\033[0m", sgr_fg(i) + 10);
    }
    format!(
        "#!/bin/sh\nprintf '{fg}\\r\\n{bg}\\r\\n'\necho painted > \"$TC_PAINTED\"\nexec sleep 600\n"
    )
}

/// The SGR foreground code of ANSI colour `i`; the background code is ten more.
fn sgr_fg(i: usize) -> usize {
    if i < 8 {
        30 + i
    } else {
        90 + i - 8
    }
}

/// The row of background cells the pixels are sampled from.
const TC_BG_ROW: f64 = 1.0;

/// How far inside each cell's edges the sample starts, so the antialiased seams between
/// fractional cells stay out of it.
const TC_CELL_INSET_PX: f64 = 3.0;

/// The share of a cell's inset interior that must be exactly the expected colour.
const TC_MIN_CELL_FRACTION: f64 = 0.90;

/// The script, the staged `init.lua` directory, and the evidence, all under `XDG_RUNTIME_DIR`
/// so no sidebar row draws them.
const TC_SHELL_NAME: &str = "wolf_gnome_tc_shell.sh";
const TC_STAGE_NAME: &str = "wolf_gnome_tc_runtime";
const TC_THEME_DARK_NAME: &str = "wolf_gnome_tc_theme_dark.txt";
const TC_THEME_LIGHT_NAME: &str = "wolf_gnome_tc_theme_light.txt";
const TC_PAINTED_DARK_NAME: &str = "wolf_gnome_tc_painted_dark.txt";
const TC_PAINTED_LIGHT_NAME: &str = "wolf_gnome_tc_painted_light.txt";
const TC_PNG_DARK_NAME: &str = "wolf_gnome_tc_dark.png";
const TC_PNG_LIGHT_NAME: &str = "wolf_gnome_tc_light.png";

/// The config and state directory suffixes of the two launches.
const TC_DARK_SUFFIX: &str = "-tc-dark";
const TC_LIGHT_SUFFIX: &str = "-tc-light";

/// The TERMINAL-COLOURS scenario: two launches in ONE `cage`, each with its own
/// `WOLF_CONFIG_DIR`, whose `appearance` is `dark` then `light`, and its own
/// `WOLF_STATE_DIR`, so the second restores nothing of the first. Each reads back
/// `g:colors_name` and `&background`, types `:terminal` to open a session on `SHELL`, waits
/// for the script's marker, settles one publish and paint, and captures the output.
///
/// A launch ends with a `kill`, not `:qa!`: the terminal pane has the keys, so a typed `:qa!`
/// would reach the shell.
///
/// ## Exit codes
///
/// * `11` / `21` — run 1's / run 2's app died before it was ready for input.
/// * `12` / `22` — its theme readback never appeared.
/// * `13` / `23` — its shell never wrote the marker: no session opened on `:terminal`.
/// * `40` — a launch never became ready (the shared convention).
const TC_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
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
    echo "wolf-gnome terminal-colours smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

stop_app() {
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
    app_pid=
}

# RUN 1 -- appearance: dark.
WOLF_CONFIG_DIR="$CONFIG_DIR_DARK"
WOLF_STATE_DIR="$STATE_DIR_DARK"
TC_PAINTED="$PAINTED_DARK"
export WOLF_CONFIG_DIR WOLF_STATE_DIR TC_PAINTED

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome terminal-colours smoke run 1 (appearance: dark)"

wtype -- ":call writefile([get(g:, 'colors_name', ''), &background], '$THEME_DARK')"
wtype -k Return
wait_file "$THEME_DARK" || exit 12
wtype -- ":terminal"
wtype -k Return
wait_file "$PAINTED_DARK" || exit 13
sleep "$SETTLE"
grim "$PNG_DARK"
stop_app

# RUN 2 -- appearance: light, same cwd, same window.
WOLF_CONFIG_DIR="$CONFIG_DIR_LIGHT"
WOLF_STATE_DIR="$STATE_DIR_LIGHT"
TC_PAINTED="$PAINTED_LIGHT"
export WOLF_CONFIG_DIR WOLF_STATE_DIR TC_PAINTED

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 21 "wolf-gnome terminal-colours smoke run 2 (appearance: light)"

wtype -- ":call writefile([get(g:, 'colors_name', ''), &background], '$THEME_LIGHT')"
wtype -k Return
wait_file "$THEME_LIGHT" || exit 22
wtype -- ":terminal"
wtype -k Return
wait_file "$PAINTED_LIGHT" || exit 23
sleep "$SETTLE"
grim "$PNG_LIGHT"
stop_app

exit 0
"#;

/// The output pixels of terminal cell `(col, row)`'s interior, inset by [`TC_CELL_INSET_PX`].
///
/// Derived, with no pixel literal: a terminal pane's canvas paints at the pane's grid rect,
/// `editor_inset::grid_rect` over the pane area at [`OVERLAY_ORIGIN_X_EXPECTED`] and
/// [`OVERLAY_ORIGIN_Y_EXPECTED`] with the statusbar `pane_view::build` measures, rounded to
/// whole pixels as `TerminalCanvas::paint` rounds it, and uncapped, as a terminal pane is full
/// width. The cap cannot bind at this geometry in any case, which
/// `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` asserts.
fn tc_cell_rect(col: usize, row: f64) -> SampleRect {
    let cell = cell_metrics_for_smoke();
    let (origin_x, origin_y) = tc_grid_origin();
    let col = f64::from(u32::try_from(col).expect("a column index fits in a u32"));
    let left = origin_x + col * cell.cell_w + TC_CELL_INSET_PX;
    let top = origin_y + row * cell.cell_h + TC_CELL_INSET_PX;
    SampleRect {
        x0: pixel_index(left, "a terminal cell's left edge"),
        x1: pixel_index(
            left + cell.cell_w - 2.0 * TC_CELL_INSET_PX,
            "a terminal cell's right edge",
        ),
        y0: pixel_index(top, "a terminal cell's top edge"),
        y1: pixel_index(
            top + cell.cell_h - 2.0 * TC_CELL_INSET_PX,
            "a terminal cell's bottom edge",
        ),
    }
}

/// The terminal canvas's origin in output pixels; see [`tc_cell_rect`].
fn tc_grid_origin() -> (f64, f64) {
    let cell = cell_metrics_for_smoke();
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );
    // The rect's x and y do not depend on the pane's height, so the area's full height serves.
    let pane_w =
        exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED") - OVERLAY_ORIGIN_X_EXPECTED;
    let pane_h =
        exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED") - OVERLAY_ORIGIN_Y_EXPECTED;
    let rect = wolf_gnome::editor_inset::grid_rect(
        pane_w,
        pane_h,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
        wolf_gnome::editor_inset::column_cap(true, cell.cell_w),
    );
    (
        OVERLAY_ORIGIN_X_EXPECTED + rect.x.round(),
        OVERLAY_ORIGIN_Y_EXPECTED + rect.y.round(),
    )
}

/// The most common colour in `rect`, and its share, for the printed table.
fn dominant(capture: &Capture, rect: SampleRect) -> (Rgb8, f64) {
    let mut counts: std::collections::BTreeMap<Rgb8, usize> = std::collections::BTreeMap::new();
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            *counts.entry(capture.pixel(x, y)).or_default() += 1;
        }
    }
    let (colour, count) = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .unwrap_or(((0, 0, 0), 0));
    (
        colour,
        band_fraction(count, rect.pixels(), "a terminal cell's dominant colour"),
    )
}

/// Checks one launch's theme readback and its capture's 16 row-1 cells against
/// `background`'s palette, printing the table of expected and measured colours first.
fn assert_launch(
    smoke: &str,
    lighthouse: &Lighthouse,
    background: Background,
    theme: &Path,
    png: &Path,
) {
    let name = background.name();

    // Precondition: the vendored Lighthouse is active on the background this launch is for.
    let raw = read_readback(theme, smoke);
    let lines: Vec<&str> = raw.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        ["lighthouse", name],
        "{smoke}: the {name} launch's Neovim reports `g:colors_name` and `&background` as \
         {lines:?}, not [\"lighthouse\", {name:?}]. An empty name means `init.lua` did not find \
         the staged `lighthouse/` beside it (WOLF_INIT_LUA); the other background means the \
         configured appearance did not reach Neovim, which \
         `configured_appearance_reaches_neovims_background_under_cage` covers. The pixels below \
         would then fail for reasons that are not the terminal's."
    );

    let expected = lighthouse.ansi(background);
    let keys = lighthouse.ansi_keys();
    let default_bg = lighthouse.colour(background, "background");
    assert!(
        !expected.contains(&default_bg),
        "{smoke}: Lighthouse's {name} `background` #{default_bg:06x} is one of its ANSI colours \
         {expected:x?}, so a cell the canvas left at the pane's default would pass for a \
         painted one. Pick another fixture before trusting this smoke."
    );

    assert_png_present(smoke, png);
    let capture = decode_capture(png, smoke, &format!("the {name} capture"));
    let mut table = format!(
        "{smoke}: {name} launch, row-1 cells against {}:\n\
         index  sgr      key                expected  dominant  share   exact share\n",
        lighthouse.path().display()
    );
    let mut wrong = Vec::new();
    for (i, &want) in expected.iter().enumerate() {
        let rect = tc_cell_rect(i, TC_BG_ROW);
        let exact = band_fraction(
            capture.count_exactly(rect, rgb8(want)),
            rect.pixels(),
            "a terminal cell's expected colour",
        );
        let ((r, g, b), share) = dominant(&capture, rect);
        table.push_str(&format!(
            "{i:>5}  {:>3}/{:<3}  {:<17}  #{want:06x}   #{r:02x}{g:02x}{b:02x}   {share:.3}   \
             {exact:.3}\n",
            sgr_fg(i),
            sgr_fg(i) + 10,
            keys[i]
        ));
        if exact < TC_MIN_CELL_FRACTION {
            wrong.push(i);
        }
    }
    eprintln!("{table}");
    assert!(
        wrong.is_empty(),
        "{smoke}: in the {name} launch, ANSI colours {wrong:?} paint under {TC_MIN_CELL_FRACTION} \
         of their cell's inset interior in Lighthouse's {name} colour. A dominant colour from no \
         Lighthouse palette means the session kept libghostty-vt's built-in palette, so \
         g:terminal_color_N was not sent or not laid over it; the other background's values \
         mean the appearance did not reach the session; channels in another order mean the \
         canvas converts the colour wrongly; #{default_bg:06x}, the default background, means \
         the cell's background is not painted or the cells are not where the derived geometry \
         puts them. PNG kept at {}.\n{table}",
        png.display()
    );
}

/// **A terminal pane paints Lighthouse's 16 ANSI colours, dark and light, under cage.**
///
/// The checkout binary runs over a staged `init.lua` beside a copy of the vendored Lighthouse
/// (`WOLF_INIT_LUA`), so `init.lua` activates the real colorscheme, and the front-end pushes
/// each launch's configured appearance. The expected colours come from the pinned
/// `lighthouse.lua` through [`Lighthouse::from_env`], read in this process.
///
/// ## Assertions, per launch
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 1 | the scenario exits 0 | precondition | none |
/// | 2 | Neovim reports `lighthouse` and the launch's background | precondition | none |
/// | 3 | Lighthouse's `background` is none of its ANSI colours | precondition | none |
/// | 4 | each row-1 cell's inset interior is at least [`TC_MIN_CELL_FRACTION`] exactly its ANSI colour | discriminator | `wolf_setup_theme_tracking`'s `terminal_color(n)` returns `-1` first; `canvas.rs`' `set_source(cr, paint.background)` swaps `r` and `b` |
#[test]
fn terminal_pane_paints_lighthouses_ansi_colours_under_cage() {
    const SMOKE: &str = "terminal_pane_paints_lighthouses_ansi_colours_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let lighthouse = Lighthouse::from_env(SMOKE);

    // The captive init.lua beside the vendored colorscheme, both copied by content.
    let stage = runtime_dir.join(TC_STAGE_NAME);
    let colors = stage.join("lighthouse").join("colors");
    std::fs::create_dir_all(&colors)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to create {}: {e}", colors.display()));
    let checkout_init = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/nvim/init.lua");
    let init_lua = stage.join("init.lua");
    std::fs::copy(&checkout_init, &init_lua).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to copy {} to {}: {e}",
            checkout_init.display(),
            init_lua.display()
        )
    });
    std::fs::copy(lighthouse.path(), colors.join("lighthouse.lua")).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to copy {} into {}: {e}",
            lighthouse.path().display(),
            colors.display()
        )
    });

    let shell = runtime_dir.join(TC_SHELL_NAME);
    std::fs::write(&shell, colour_shell())
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to write {}: {e}", shell.display()));
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap_or_else(
            |e| {
                panic!(
                    "{SMOKE}: failed to make {} executable: {e}",
                    shell.display()
                )
            },
        );
    }

    // Precondition: the derived grid origin lies in the band the probe-click measurement allows.
    let (origin_x, origin_y) = tc_grid_origin();
    let cell = cell_metrics_for_smoke();
    for (axis, derived, measured, cell_size) in [
        ("x", origin_x, PANE_ORIGIN_X_EXPECTED, cell.cell_w),
        ("y", origin_y, PANE_ORIGIN_Y_EXPECTED, cell.cell_h),
    ] {
        assert!(
            (derived - measured).abs() <= PANE_ORIGIN_TOLERANCE_PX + cell_size,
            "{SMOKE}: the grid origin derived from the pane area, {axis} = {derived}, is outside \
             the band around PANE_ORIGIN_{}_EXPECTED = {measured} (tolerance \
             {PANE_ORIGIN_TOLERANCE_PX} plus one {cell_size} px cell). The pane chrome moved: \
             re-check OVERLAY_ORIGIN_*_EXPECTED and the statusbar before the cell rects.",
            axis.to_uppercase()
        );
    }

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);
    let config_dark =
        smoke_config_dir_at(&cwd, TC_DARK_SUFFIX, SMOKE_FONT_PT, Background::Dark.name());
    let config_light = smoke_config_dir_at(
        &cwd,
        TC_LIGHT_SUFFIX,
        SMOKE_FONT_PT,
        Background::Light.name(),
    );
    let state_dark = smoke_state_dir_at(&cwd, TC_DARK_SUFFIX);
    let state_light = smoke_state_dir_at(&cwd, TC_LIGHT_SUFFIX);

    let theme_dark = runtime_dir.join(TC_THEME_DARK_NAME);
    let theme_light = runtime_dir.join(TC_THEME_LIGHT_NAME);
    let painted_dark = runtime_dir.join(TC_PAINTED_DARK_NAME);
    let painted_light = runtime_dir.join(TC_PAINTED_LIGHT_NAME);
    let png_dark = runtime_dir.join(TC_PNG_DARK_NAME);
    let png_light = runtime_dir.join(TC_PNG_LIGHT_NAME);
    for path in [
        &theme_dark,
        &theme_light,
        &painted_dark,
        &painted_light,
        &png_dark,
        &png_light,
    ] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(TC_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("WOLF_INIT_LUA", &init_lua)
        .env("SHELL", &shell)
        .env("CONFIG_DIR_DARK", &config_dark)
        .env("CONFIG_DIR_LIGHT", &config_light)
        .env("STATE_DIR_DARK", &state_dark)
        .env("STATE_DIR_LIGHT", &state_light)
        .env("THEME_DARK", &theme_dark)
        .env("THEME_LIGHT", &theme_light)
        .env("PAINTED_DARK", &painted_dark)
        .env("PAINTED_LIGHT", &painted_light)
        .env("PNG_DARK", &png_dark)
        .env("PNG_LIGHT", &png_light)
        .status()
        .expect("failed to spawn `cage` for the headless terminal-colours scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11 or 21: run 1's \
         (dark) or run 2's (light) app died before it was ready for input. Exit 12 or 22: its \
         `g:colors_name` readback never appeared. Exit 13 or 23: its shell never wrote the marker, \
         so no session opened on `:terminal` or SHELL {} never ran. Exit 40: a launch never \
         became ready.",
        shell.display()
    );

    // --- Assertions 2 to 4, per launch ---------------------------------------
    assert_launch(SMOKE, &lighthouse, Background::Dark, &theme_dark, &png_dark);
    assert_launch(
        SMOKE,
        &lighthouse,
        Background::Light,
        &theme_light,
        &png_light,
    );
}
