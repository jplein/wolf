//! Configuration smokes: `wolf-config.yaml`'s font size and appearance reaching the grid and
//! Neovim, and the content-width cap with the full-width toggle that lifts it.

use super::*;

// --- The configured font size reaches the grid ------------------------------

/// The `font-size` run 1's config directory carries, in points.
///
/// Both sizes are chosen well apart so the resulting column counts cannot be
/// confused for a rounding wobble: DejaVu Sans Mono's advance is about `0.80` px
/// per point at 96 dpi, so 20 pt and 40 pt give cells roughly 16 px and 32 px wide
/// and therefore roughly twice as many columns at the small size as at the large
/// one. Neither equals [`SMOKE_FONT_PT`], so a launch that fell back to the helper's
/// own seeded config would report the same width for both runs and assertion 2
/// would fire.
const CONFIG_GRID_SMALL_FONT_PT: &str = "20";

/// The `font-size` run 2's config directory carries, in points. See
/// [`CONFIG_GRID_SMALL_FONT_PT`].
const CONFIG_GRID_LARGE_FONT_PT: &str = "40";

/// Name suffix of run 1's config directory sibling.
const CONFIG_GRID_SMALL_SUFFIX: &str = "-small";

/// Name suffix of run 2's config directory sibling.
const CONFIG_GRID_LARGE_SUFFIX: &str = "-large";

/// File run 1's Neovim writes its `winwidth(0)` into.
///
/// Both readbacks live under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so
/// the app's own `FileSearch` never lists them and the file-tree sidebar is
/// identical in both runs — the sidebar shares the window with the editor pane, so
/// a row count that differed between runs would be a second variable in a
/// comparison that must have exactly one.
const CONFIG_GRID_SMALL_READBACK_NAME: &str = "wolf_gnome_config_grid_small.txt";

/// File run 2's Neovim writes its `winwidth(0)` into. See
/// [`CONFIG_GRID_SMALL_READBACK_NAME`].
const CONFIG_GRID_LARGE_READBACK_NAME: &str = "wolf_gnome_config_grid_large.txt";

/// The smallest column difference between the two runs that this smoke accepts.
///
/// Assertion 2 (a strict inequality) is the discriminating one; this bound is what
/// stops it being satisfied by a one-column wobble from cell-width rounding or from
/// a divider landing a pixel differently. The measured separation is roughly a
/// factor of two — tens of columns at the window sizes `cage` provides — so five is
/// far below what a working build produces and far above what noise can manufacture.
const CONFIG_GRID_MIN_COLUMN_GAP: i64 = 5;

/// The two-launch CONFIGURED-FONT-SIZE scenario, run as one `sh -c` string inside
/// ONE `cage`, structurally the relaunch smokes' shape.
///
/// Each launch exports its own `WOLF_CONFIG_DIR`, deliberately overriding the one
/// [`cage_command`] sets, and reads `winwidth(0)` back out of the captive Neovim
/// into its own file. The readback is `winwidth()` and never
/// `nvim_win_get_width()`: under `ext_multigrid` the former reports the GRID width
/// the front-end drives from the measured cell size, while the latter reports the
/// layout width, which is blind to it.
///
/// Exit codes, each naming one cause: `11` run 1's app died before it was
/// ready for input; `12` run 1 was still alive after `:qa!`; `13` run 1 exited non-zero;
/// `14` run 1's width readback never appeared; `15`–`18` the same four for run 2.
const CONFIG_GRID_SCENARIO: &str = r#"
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
    echo "wolf-gnome config-grid smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# $1 is the exit code for "still alive after :qa!", $2 for "exited non-zero".
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
        echo "wolf-gnome config-grid smoke: app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome config-grid smoke: app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

# =========================================================================
# RUN 1 -- the SMALL configured font size.
#
# This export deliberately overrides the WOLF_CONFIG_DIR `cage_command` set: the
# helper's directory pins the smokes' shared font size, and this smoke is the one
# where the CONFIGURED size has to be what decides. WOLF_FONT_PT is removed on the
# Rust side for the same reason.
# =========================================================================
WOLF_CONFIG_DIR="$CONFIG_DIR_SMALL"
export WOLF_CONFIG_DIR

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome config-grid smoke"

# `winwidth(0)`, NEVER `nvim_win_get_width()`: the former is the GRID width the
# front-end drives from the measured cell size.
wtype -- ":call writefile([printf('%d', winwidth(0))], '$WIDTH_SMALL')"
wtype -k Return
wait_file "$WIDTH_SMALL" || exit 14

quit_app 12 13

# =========================================================================
# RUN 2 -- the LARGE configured font size, same cwd, same window.
# =========================================================================
WOLF_CONFIG_DIR="$CONFIG_DIR_LARGE"
export WOLF_CONFIG_DIR

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 15 "wolf-gnome config-grid smoke"

wtype -- ":call writefile([printf('%d', winwidth(0))], '$WIDTH_LARGE')"
wtype -k Return
wait_file "$WIDTH_LARGE" || exit 18

quit_app 16 17

exit 0
"#;

/// The `font-size` in `wolf-config.yaml` reaches the editor grid.
///
/// ## What it proves, and why it is not an OCR assertion
///
/// That the app READS the shared `wolf-config.yaml` and measures its cell from
/// what it found there. OCR reads text, not size: a change in font size is visible
/// to it only indirectly, through wrapping, which is a fragile thing to assert on.
/// Two launches at two configured sizes, each writing `winwidth(0)` into its own
/// file, yield an integer strict inequality instead — a value a config-ignoring
/// build cannot produce, because both of its launches would measure the same cell
/// and report the same number of columns.
///
/// Both launches run inside ONE `cage` instance, so the smoke holds one cage slot
/// and no second compositor is started.
///
/// ## The one smoke that removes `WOLF_FONT_PT`
///
/// Every other smoke exports it, and it WINS over the file's `font-size` by design
/// (`config_seam::resolve_font_pt`, matching `CellMetrics.defaultPointSize` on
/// macOS). Here the configured size has to be what decides, so the override is
/// removed on purpose — with the helper's four remaining variables left in place,
/// since `cage_command` is still the only route to the compositor.
///
/// ## What it does NOT prove
///
/// That a configured `font-family` reaches a pixel: no smoke changes the family,
/// because doing so would change every glyph on the page and put the OCR
/// calibration at risk for no added confidence — `tests/paint_golden.rs` covers
/// family resolution headlessly. And not that a configured `line-height` reaches a
/// pixel: its arithmetic is unit-tested and [`cell_metrics_for_smoke`] demonstrably
/// feeds it, but no capture reads a cell height.
///
/// ## Red-run reverts
///
/// In `app.rs::measure_metrics`, ignore the config and pass literals
/// (`14.0` / `"DejaVu Sans Mono"` / `100.0`) to `CellMetrics::for_font`: both
/// launches then report the same width and assertion 2 fires. A second, INDEPENDENT
/// revert — making `resolve_font_pt` prefer `config_font_size` over the override —
/// leaves this smoke green, since the override is absent here anyway. That is worth
/// recording as a known non-discriminator: the precedence rule is pinned by
/// `config_seam`'s own tests, not by this smoke.
#[test]
fn config_font_size_changes_the_grid_under_cage() {
    const SMOKE: &str = "config_font_size_changes_the_grid_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by both launches. The filler files are the usual
    // sidebar population; nothing here is OCR'd, so no fixture carries a term.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    // The font size is this smoke's one variable, so both runs pin the appearance too.
    let config_small = smoke_config_dir_at(
        &cwd,
        CONFIG_GRID_SMALL_SUFFIX,
        CONFIG_GRID_SMALL_FONT_PT,
        SMOKE_APPEARANCE,
    );
    let config_large = smoke_config_dir_at(
        &cwd,
        CONFIG_GRID_LARGE_SUFFIX,
        CONFIG_GRID_LARGE_FONT_PT,
        SMOKE_APPEARANCE,
    );
    assert_ne!(
        config_small,
        config_large,
        "{SMOKE}: both runs were pointed at the SAME config directory {}, so they would read the \
         same font-size and the strict inequality below could never hold. The two suffixes \
         ({CONFIG_GRID_SMALL_SUFFIX:?} and {CONFIG_GRID_LARGE_SUFFIX:?}) must differ.",
        config_small.display()
    );

    let width_small = runtime_dir.join(CONFIG_GRID_SMALL_READBACK_NAME);
    let width_large = runtime_dir.join(CONFIG_GRID_LARGE_READBACK_NAME);
    for path in [&width_small, &width_large] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(CONFIG_GRID_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("CONFIG_DIR_SMALL", &config_small)
        .env("CONFIG_DIR_LARGE", &config_large)
        .env("WIDTH_SMALL", &width_small)
        .env("WIDTH_LARGE", &width_large);

    // `WOLF_FONT_PT` beats the configured `font-size`, so left set it makes both launches measure alike
    // and fail assertion 2 for the wrong reason. Removed here only, on purpose.
    cmd.env_remove("WOLF_FONT_PT");

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless configured-font-size scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage configured-font-size scenario exited unsuccessfully ({status}). Exit \
         11: run 1's app died before it was ready for input — with WOLF_FONT_PT removed, a panic in \
         the config path (`load_config`, `resolve_font_pt`, or a non-positive size reaching \
         `for_font`) is the first thing to look at. Exit 12: run 1 was still alive after `:qa!`. \
         Exit 13: run 1 exited NON-ZERO — it went away, but not cleanly. Exit 14: run 1's width \
         readback never appeared at {}. Exit 15/16/17/18: the same four for run 2, whose readback \
         is at {}.",
        width_small.display(),
        width_large.display()
    );

    // Precondition: a missing or unparseable file means the app never answered, not that the config was
    // ignored; the two need different fixes.
    let small_raw = read_trimmed(&width_small, SMOKE);
    let large_raw = read_trimmed(&width_large, SMOKE);
    let small_cols = parse_readback_int(&small_raw, &width_small, &small_raw, SMOKE);
    let large_cols = parse_readback_int(&large_raw, &width_large, &large_raw, SMOKE);
    for (label, cols, font_pt) in [
        ("small", small_cols, CONFIG_GRID_SMALL_FONT_PT),
        ("large", large_cols, CONFIG_GRID_LARGE_FONT_PT),
    ] {
        assert!(
            cols > 0,
            "{SMOKE}: the {label} run (font-size {font_pt}) reported winwidth(0) = {cols}. A \
             non-positive column count means the pane never bound a grid, so the comparison below \
             would be between a real measurement and a placeholder."
        );
    }

    // --- Assertion 2: the discriminating one ----------------------------------
    assert!(
        small_cols > large_cols,
        "{SMOKE}: the {CONFIG_GRID_SMALL_FONT_PT} pt run reported {small_cols} columns and the \
         {CONFIG_GRID_LARGE_FONT_PT} pt run reported {large_cols}. A smaller font means a narrower \
         cell and therefore MORE columns in the same pane, so the small run must report strictly \
         more. EQUAL counts are the signature of a build that never read the config: \
         `app::measure_metrics` is passing a literal family/size to `CellMetrics::for_font` \
         instead of the loaded `WolfConfig`, or `load_config` resolved no preferences directory \
         and silently degraded to the built-in defaults for both runs. INVERTED counts would mean \
         the two config directories are wired to the wrong launches."
    );

    // --- Assertion 3: the sanity bound ----------------------------------------
    let gap = small_cols - large_cols;
    assert!(
        gap >= CONFIG_GRID_MIN_COLUMN_GAP,
        "{SMOKE}: the two runs differ by only {gap} column(s) ({small_cols} at \
         {CONFIG_GRID_SMALL_FONT_PT} pt vs {large_cols} at {CONFIG_GRID_LARGE_FONT_PT} pt), below \
         the {CONFIG_GRID_MIN_COLUMN_GAP}-column floor. Doubling the point size roughly halves the \
         column count, so a difference this small says the size reaching the metrics is not the \
         one the config names — a clamped or partially-applied value rather than an ignored one. \
         Assertion 2 alone would accept it, which is exactly why this bound exists."
    );
}

// --- The configured appearance reaches Neovim's `background` -----------------

/// The `appearance` run 1's config directory carries.
///
/// Deliberately its OWN const rather than [`SMOKE_APPEARANCE`], even though the
/// two spell the same word today. This smoke has to drive BOTH forced values
/// whatever the rest of the file happens to be pinned to, so re-pinning the shared
/// value must not quietly turn one of these two launches into a copy of the other.
pub(crate) const APPEARANCE_FORCED_DARK: &str = "dark";

/// The `appearance` run 2's config directory carries. See
/// [`APPEARANCE_FORCED_DARK`].
pub(crate) const APPEARANCE_FORCED_LIGHT: &str = "light";

/// Name suffix of run 1's config directory sibling.
const APPEARANCE_DARK_SUFFIX: &str = "-appearance-dark";

/// Name suffix of run 2's config directory sibling.
const APPEARANCE_LIGHT_SUFFIX: &str = "-appearance-light";

/// File run 1's Neovim writes `&background` into.
///
/// Both readbacks live under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so
/// the app's own `FileSearch` never lists them and neither name can reach the tree
/// sidebar, the header bar or an `:e` echo. Nothing here is OCR'd in any case —
/// the assertion reads the CONTENT of a file the captive Neovim wrote about its
/// own option, and a file name is not on that path at all.
const APPEARANCE_DARK_READBACK_NAME: &str = "wolf_gnome_appearance_background_1.txt";

/// File run 2's Neovim writes `&background` into. See
/// [`APPEARANCE_DARK_READBACK_NAME`].
const APPEARANCE_LIGHT_READBACK_NAME: &str = "wolf_gnome_appearance_background_2.txt";

/// The two-launch CONFIGURED-APPEARANCE scenario, run as one `sh -c` string inside
/// ONE `cage`, structurally [`CONFIG_GRID_SCENARIO`]'s shape.
///
/// Each launch exports its own `WOLF_CONFIG_DIR`, deliberately overriding the one
/// [`cage_command`] sets, and reads `&background` back out of the captive Neovim
/// into its own file. `&background` and not a highlight-group colour: the option
/// is the value the front-end pushes, it is a plain string Neovim will write out,
/// and the colours it selects are the colorscheme's business rather than this
/// crate's.
///
/// Exit codes are PER LAUNCH, [`CONFIG_GRID_SCENARIO`]'s scheme exactly: `11` run
/// 1's app died before it was ready for input; `12` run 1 was still alive after `:qa!`;
/// `13` run 1 exited non-zero; `14` run 1's `&background` readback never appeared
/// (or stayed empty); `15`–`18` the same four for run 2.
///
/// Eight codes and not five, because a code shared between the launches folds two
/// causes into one: "an app died before it was ready for input" is a different
/// investigation depending on whether it was the launch reading a forced `dark` or
/// the one reading a forced `light`, and the convention
/// `crates/wolf-gnome/tests/README.md` states is about the CODE carrying the
/// meaning rather than about the cause being recoverable by other means. `12` and
/// `13` are separate for the reason that convention gives directly: a process that
/// panicked in the GTK main loop and one that shut down cleanly are equally GONE,
/// and only the status tells them apart.
///
/// Each of the three process-level codes additionally prints the run's LABEL on
/// stderr, which costs nothing and makes a truncated log readable without the
/// code table.
const APPEARANCE_SCENARIO: &str = r#"
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
    echo "wolf-gnome appearance smoke: readback file $f never appeared (or stayed empty)." >&2
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
        echo "wolf-gnome appearance smoke: $3 app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome appearance smoke: $3 app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

# =========================================================================
# RUN 1 -- appearance: dark.
#
# This export deliberately overrides the WOLF_CONFIG_DIR `cage_command` set: the
# helper's directory pins the shared appearance, and this smoke is the one where
# the CONFIGURED appearance has to be what decides.
# =========================================================================
WOLF_CONFIG_DIR="$CONFIG_DIR_DARK"
export WOLF_CONFIG_DIR

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome appearance smoke run 1 (appearance: dark)"

# `&background` is a global option, so no window or buffer has to be current for
# this to be the value the front-end pushed.
wtype -- ":call writefile([&background], '$BG_DARK')"
wtype -k Return
wait_file "$BG_DARK" || exit 14

quit_app 12 13 "run 1 (appearance: dark)"

# =========================================================================
# RUN 2 -- appearance: light, same cwd, same window.
# =========================================================================
WOLF_CONFIG_DIR="$CONFIG_DIR_LIGHT"
export WOLF_CONFIG_DIR

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 15 "wolf-gnome appearance smoke run 2 (appearance: light)"

wtype -- ":call writefile([&background], '$BG_LIGHT')"
wtype -k Return
wait_file "$BG_LIGHT" || exit 18

quit_app 16 17 "run 2 (appearance: light)"

exit 0
"#;

/// The `appearance` in `wolf-config.yaml` reaches the captive Neovim's
/// `background` option: `dark` gives `dark`, `light` gives `light`.
///
/// ## What it proves, and why the observable is an option rather than a pixel
///
/// Two launches inside ONE `cage` instance, two seeded `WOLF_CONFIG_DIR`s, no
/// capture at all. Each launch asks its Neovim to write `&background` into its own
/// file, and the assertions are two exact string equalities.
///
/// OCR reads no colour, so the appearance cannot be read off a screenshot at all —
/// but the readback route is not a consolation prize here, it is stronger:
/// `&background` is the exact value `WolfCore::set_background` pushes, written out
/// by the process that received it. The push rides the ordered input channel and
/// is enqueued at launch, before the window is built, so it is applied ahead of
/// every keystroke the scenario later types; the settle is margin, not a race.
///
/// ## THE PAIR IS WHAT DISCRIMINATES, AND NEITHER HALF DOES IT ALONE
///
/// This is the whole design of the smoke, and it is why a future edit must not
/// drop one launch as redundant:
///
/// - Assertion 1 (`dark`) is satisfied by a build that pushes NOTHING. `dark` is
///   Neovim's own default under `--embed`, so a front-end that never called
///   `set_background` produces exactly the value assertion 1 demands.
/// - Assertion 2 (`light`) is satisfied by a build that ALWAYS pushes light —
///   `appearance_decision` returning a constant, or `apply_at_launch` pushing a
///   literal.
///
/// Together they exclude both, because no single constant satisfies the two.
///
/// The premise under assertion 1 is stated rather than assumed: nothing rests on
/// Neovim's default really being `dark`, because that premise only makes assertion
/// 1 WEAKER. If the default were `light`, assertion 1 would become discriminating
/// too and this smoke would be strictly stronger than described.
///
/// ## The named falsifiers
///
/// | revert | what happens |
/// | --- | --- |
/// | delete the `core.set_background(decided.dark)` call in `appearance::apply_at_launch` | run 2 reads `dark` (Neovim's default) and assertion 2 fires; the fast suite stays GREEN, which is the diagnostic — the rule is right and nothing carries it out |
/// | return `dark: true` for `Appearance::Light` in `appearance::appearance_decision` | run 2 reads `dark` as well, AND `appearance::tests::appearance_decision_table` goes red. A red smoke over a red fast suite is the rule itself being wrong; over a green one it is the applier |
///
/// ## What it does NOT prove
///
/// Three entries for `tests/README.md`'s "What is still unproven", named here so
/// they are not lost.
///
/// **The `auto` arm.** Under `cage` there is no settings portal and no xsettings
/// bridge, so `adw::StyleManager::is_dark()` is constant for a whole run and no
/// smoke can vary it. `auto` is table-tested in `appearance::appearance_decision`
/// and reached by nothing here.
///
/// **The live OS-toggle re-push.** `connect_dark_notify` fires only on a real
/// change of libadwaita's `dark` property, which nothing under `cage` can produce.
/// That `observe_os` is decided correctly is table-tested; that the handler is
/// ever INVOKED is unobserved on this platform.
///
/// **That any colour reaches a pixel.** OCR reads no colour — the same shape as
/// the status bar's active treatment and the popup's reverse video. This smoke
/// says nothing about the GTK chrome's colour scheme, which
/// `appearance_decision` pins as a value and `apply_at_launch` demonstrably passes
/// to `set_color_scheme`, and nothing about the grid's colours, which are the
/// captive Neovim's colorscheme responding to the option this smoke reads. That
/// scoping is exact and stays; the colour claim belongs to a SIBLING —
/// `preview_document_follows_the_configured_appearance_under_cage` decodes
/// pixels instead of OCR'ing, and is the one test here that observes
/// `set_color_scheme`.
///
/// ## The conventions it obeys
///
/// A fresh temp working directory ([`make_temp_cwd`], mandatory — the captive
/// Neovim's swap directory is keyed on it); [`require_smoke_env`]'s cage slot and runtime directory for the whole body; one
/// `sh -c` string through [`cage_command`], the single environment helper and the
/// only place the compositor is spawned; and eight exit codes naming eight causes,
/// four per launch, so no code covers both — see [`APPEARANCE_SCENARIO`] for why
/// the per-launch split is not merely tidiness.
/// It asserts no OCR term and captures no PNG, so it contributes no row to any
/// fixture-name guard — the treatment the divider-relayout, column-cap and
/// clipboard smokes already get.
#[test]
fn configured_appearance_reaches_neovims_background_under_cage() {
    const SMOKE: &str = "configured_appearance_reaches_neovims_background_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by both launches. The filler files are the usual
    // sidebar population; nothing here is OCR'd, so no fixture carries a term.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    // Both launches run at SMOKE_FONT_PT: the appearance is the ONE variable this
    // smoke may have.
    let config_dark = smoke_config_dir_at(
        &cwd,
        APPEARANCE_DARK_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
    );
    let config_light = smoke_config_dir_at(
        &cwd,
        APPEARANCE_LIGHT_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_LIGHT,
    );
    assert_ne!(
        config_dark,
        config_light,
        "{SMOKE}: both runs were pointed at the SAME config directory {}, so they would read the \
         same appearance and one of the two assertions below could never hold. The two suffixes \
         ({APPEARANCE_DARK_SUFFIX:?} and {APPEARANCE_LIGHT_SUFFIX:?}) must differ.",
        config_dark.display()
    );

    // Precondition: `appearance` has a serde default of `auto`, so a harness that stopped writing it
    // would put both launches in `auto` and fail as a front-end claim.
    for (label, dir, want) in [
        ("run 1", &config_dark, APPEARANCE_FORCED_DARK),
        ("run 2", &config_light, APPEARANCE_FORCED_LIGHT),
    ] {
        let path = dir.join(CONFIG_FILE_NAME);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{SMOKE}: could not read the {label} config file {} that `smoke_config_dir_at` \
                 just seeded: {e}",
                path.display()
            )
        });
        let key = format!("appearance: {want}");
        assert!(
            text.contains(&key),
            "{SMOKE}: the {label} config file {} does not carry {key:?}. Its contents were:\n\
             {text}\nWithout the key `wolf_core::config` defaults the field to `auto`, so this \
             launch would resolve its appearance from a settings portal that does not exist under \
             `cage` — and BOTH launches would then report the same value, failing an assertion \
             below for a reason that has nothing to do with the front-end honouring the setting.",
            path.display()
        );
    }

    let bg_dark = runtime_dir.join(APPEARANCE_DARK_READBACK_NAME);
    let bg_light = runtime_dir.join(APPEARANCE_LIGHT_READBACK_NAME);
    for path in [&bg_dark, &bg_light] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(APPEARANCE_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("CONFIG_DIR_DARK", &config_dark)
        .env("CONFIG_DIR_LIGHT", &config_light)
        .env("BG_DARK", &bg_dark)
        .env("BG_LIGHT", &bg_light);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless configured-appearance scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage configured-appearance scenario exited unsuccessfully ({status}). Every \
         code names ONE cause in ONE launch. Exit 11: run 1's app (`appearance: \
         {APPEARANCE_FORCED_DARK}`) died before it was ready for input — a panic while resolving the \
         appearance is the first thing to look at, since `appearance::apply_at_launch` runs before \
         any widget is built and takes the whole launch with it. Exit 12: run 1 was still alive \
         after `:qa!`. Exit 13: run 1 exited NON-ZERO — it went away, but not cleanly, and that is \
         a separate code from 12 because a panic in the GTK main loop makes a process vanish \
         exactly as a clean shutdown does. Exit 14: run 1's `&background` readback never appeared \
         at {}. Exit 15/16/17/18: the same four for run 2 (`appearance: \
         {APPEARANCE_FORCED_LIGHT}`), whose readback is at {}. A run-2 code with run 1's readback \
         already written means the front-end handled the forced-dark launch and fell over on the \
         forced-light one, which is a much narrower finding than the code alone suggests.",
        bg_dark.display(),
        bg_light.display()
    );

    // --- The two assertions. NEITHER discriminates alone; see the doc comment --
    let got_dark = read_trimmed(&bg_dark, SMOKE);
    let got_light = read_trimmed(&bg_light, SMOKE);

    assert_eq!(
        got_dark, APPEARANCE_FORCED_DARK,
        "{SMOKE}: run 1 was launched with `appearance: {APPEARANCE_FORCED_DARK}` and its Neovim \
         reported `&background` = {got_dark:?}. This half of the pair is the WEAK one on purpose — \
         `{APPEARANCE_FORCED_DARK}` is what Neovim defaults to under `--embed`, so a build that \
         pushes nothing at all still satisfies it. A FAILURE here therefore means something \
         stronger than a missing push: the front-end pushed the OPPOSITE of what was configured, \
         or the readback ran in a Neovim that never received the launch push at all. Run 2's \
         assertion is the one that detects a missing push."
    );
    assert_eq!(
        got_light, APPEARANCE_FORCED_LIGHT,
        "{SMOKE}: run 2 was launched with `appearance: {APPEARANCE_FORCED_LIGHT}` and its Neovim \
         reported `&background` = {got_light:?}. THIS is the discriminating half. Reading \
         {APPEARANCE_FORCED_DARK:?} means the configured appearance never reached Neovim: either \
         `appearance::apply_at_launch` does not call `core.set_background(decided.dark)`, or \
         `appearance::appearance_decision` resolves `Appearance::Light` to `dark: true`. Those two \
         are told apart by the fast suite — `appearance::tests::appearance_decision_table` is RED \
         for the second and GREEN for the first. Anything that is neither {APPEARANCE_FORCED_DARK:?} \
         nor {APPEARANCE_FORCED_LIGHT:?} means the Ex command did not execute as typed; check for a \
         stray keystroke landing in the buffer instead of the cmdline."
    );
}

// --- The column cap, and the per-buffer override that lifts it ---------------

/// The font size THIS smoke launches at, in points, overriding [`SMOKE_FONT_PT`].
///
/// It is the whole reason the smoke is possible, and the arithmetic is the point:
///
/// At [`SMOKE_FONT_PT`] (28) the measured DejaVu Sans Mono cell is 22.4765625 px
/// wide, so `MAX_GRID_COLS` columns need 2022.89 px while one pane under `cage`
/// leaves 989 px of grid. The cap cannot bind — that is what
/// [`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`] asserts, deliberately,
/// for every other smoke in this binary.
///
/// At 10 pt the same font gives `10 * 96/72 * 0.60205 = 8.03` px per cell, so 90
/// columns need about 723 px and the cap binds with room to spare; uncapped, the same
/// 989 px pane is about 123 columns. That ~33-column separation is what makes both
/// readbacks two-sided rather than a rounding judgement.
///
/// Nothing on this page is OCR'd, so a size far too small to read is free here and
/// would be unusable anywhere else in this binary.
pub(crate) const FULLWIDTH_SMOKE_FONT_PT: &str = "10";

/// File the captive Neovim writes its `winwidth(0)` into BEFORE the toggle, i.e.
/// while the content-width cap is in force.
///
/// Both readbacks live under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so the
/// app's own `FileSearch` never lists them and neither becomes a tree-sidebar row.
/// Nothing here is OCR'd, so that is belt-and-braces rather than load-bearing — but
/// it also keeps this smoke's page identical to the other single-launch ones, which
/// is what makes a failure here about the grid and not about the fixture.
const FULLWIDTH_CAPPED_READBACK_NAME: &str = "wolf_gnome_fullwidth_capped.txt";

/// File the captive Neovim writes its `winwidth(0)` into AFTER `:WolfFullWidth`. See
/// [`FULLWIDTH_CAPPED_READBACK_NAME`].
const FULLWIDTH_FULL_READBACK_NAME: &str = "wolf_gnome_fullwidth_full.txt";

/// The single-launch COLUMN-CAP scenario, run as one `sh -c` string inside one
/// `cage`, structurally [`CONFIG_GRID_SCENARIO`]'s shape with one launch instead of
/// two.
///
/// The readback is `winwidth()` and never `nvim_win_get_width()`: under
/// `ext_multigrid` the former reports the GRID width the front-end drives from the
/// column cap, while the latter reports the layout width, which is blind to it.
///
/// Exit codes, each naming one cause: `11` the app died before it was ready for input;
/// `12` the CAPPED readback never appeared; `13` the FULL-WIDTH readback never
/// appeared; `14` the app was still alive after `:qa!`; `15` the app exited non-zero.
/// 12 and 13 are separate on purpose — the first says the app never got as far as
/// answering at all, the second says it answered once and then stopped, and the two
/// have different first suspects.
const FULLWIDTH_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# TWICE the settle, and the doubling is deliberate. The first readback follows a
# keystroke that Neovim answers by itself. The second follows a round trip with seven
# hops: :WolfFullWidth -> the command queue -> the front-end's drain on the next wake
# -> InputEvent::ToggleFullWidth -> wolf_toggle_full_width flipping b:wolf_full_width
# -> wolf_emit_full_width -> the snapshot -> the NEXT wake's sync_pane_grids issuing
# resize_grid -> Neovim's flush. Every hop is a separate main-loop turn, and the
# resize is fire-and-forget, so nothing here is observable to poll for.
TOGGLE_SETTLE=2.0
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
    echo "wolf-gnome column-cap smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# $1 is the exit code for "still alive after :qa!", $2 for "exited non-zero".
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
        echo "wolf-gnome column-cap smoke: app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome column-cap smoke: app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome column-cap smoke"

# READBACK 1 -- the grid while the content-width cap is in force.
wtype -- ":call writefile([printf('%d', winwidth(0))], '$WIDTH_CAPPED')"
wtype -k Return
wait_file "$WIDTH_CAPPED" || exit 12

# The toggle. It is typed as the Ex command rather than driven from a menu or a
# chord, deliberately: this command has neither, and `:WolfFullWidth` is the surface
# it does have (plus the command palette row, which resolves through the same
# `ui_action_for`).
wtype -- ":WolfFullWidth"
wtype -k Return
sleep "$TOGGLE_SETTLE"

# READBACK 2 -- the grid with the cap lifted for this buffer.
wtype -- ":call writefile([printf('%d', winwidth(0))], '$WIDTH_FULL')"
wtype -k Return
wait_file "$WIDTH_FULL" || exit 13

quit_app 14 15

exit 0
"#;

/// The 90-column content cap reaches the grid Neovim is given, and
/// `:WolfFullWidth` lifts it for the focused buffer.
///
/// ## What it proves, and why it needs its own font size
///
/// Two `winwidth(0)` readbacks from one launch, either side of one Ex command. The
/// first must be EXACTLY `editor_inset::MAX_GRID_COLS`, which no uncapped build can
/// produce at this pane width; the second must be strictly greater, which no build
/// that drops the toggle anywhere along its seven-hop path can produce.
///
/// It is the ONLY end-to-end proof of the cap that is possible at all, and it is
/// possible only because a smoke may choose its own font size. At the shared
/// [`SMOKE_FONT_PT`] the cap cannot bind — one pane leaves 989 px of grid and 90
/// columns need 2022.89 px — which is a premise
/// [`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`] asserts rather than
/// assumes, precisely so that no OCR smoke silently starts running in the capped
/// regime. See [`FULLWIDTH_SMOKE_FONT_PT`] for the arithmetic at 10 pt.
///
/// ## The one line that makes it mean anything
///
/// `WOLF_FONT_PT` is set AFTER [`cage_command`] has built the command, so this
/// smoke's value wins over the shared one. The seeded `WOLF_CONFIG_DIR` is left in
/// place: the override beats the file's `font-size` by design, so the config's family
/// and line-height still apply and only the size moves.
///
/// ## What it does NOT prove
///
/// **That the grid is CENTRED.** Neovim is told a column count and nothing about
/// where its grid is painted, so `winwidth(0)` is blind to the centring offset — and
/// no OCR assertion can read a horizontal offset either, since tesseract's `contains`
/// is position-agnostic. The centring is pinned headlessly, by
/// `editor_inset::grid_rect_table`'s capped rows.
///
/// **That a click inside a capped, centred grid lands in the right cell.** No pointer
/// smoke runs at a font size where the cap binds, and adding one would need a second
/// pointer calibration at a second cell size. That is
/// `editor_inset::a_capped_grid_maps_the_margins_and_its_own_edges_to_real_columns`'s
/// job, headlessly, with literal probe coordinates.
///
/// ## Red-run reverts, one per assertion
///
/// Assertion 2: in `editor_inset::grid_size_for_pane`, return `cols` instead of
/// `cap.apply(cols)` — readback 1 comes back around 123 instead of 90. MEASURED:
/// `the captive Neovim reported winwidth(0) = 123, want exactly 90`, left `123`,
/// right `90`. The uncapped count is 123 on the nose, which is the figure
/// [`FULLWIDTH_SMOKE_FONT_PT`]'s own arithmetic predicts from `floor(989 / 8.03)` —
/// a live readback confirming that constant's reasoning rather than restating it.
///
/// Assertion 3, three independent ones. In `command_action::ui_action_for`, restore
/// `Command::FullWidth => UiAction::Deferred(…)`: the toggle never becomes an action
/// and readback 2 stays 90. In `editor_inset::column_cap`, ignore the flag and always
/// answer `Some(MAX_GRID_COLS)`: the flag flips and changes nothing. And delete
/// `self.sync_pane_grids();` from `PaneInner::on_render_wake`: the flag flips, the
/// paint re-clips, but no `resize_grid` is ever issued because the widget allocation
/// did not change and `connect_resize` therefore never fires. **That third one is the
/// only falsifier anywhere for the no-allocation-change resize path**, which is the
/// mechanism a later reader is most likely to simplify away as redundant.
///
/// All three were MEASURED, and the measurement's lesson is that the panic text does
/// NOT tell them apart: each reports `winwidth(0) = 90` against a capped 90 and
/// prints the same four-cause message. What separates them is everything around it.
/// The `ui_action_for` revert — and only that one — emits the app's own
/// `wolf-gnome: command FullWidth not dispatched: …` line on stderr first. The
/// `column_cap` revert additionally reddens `editor_inset::tests::column_cap_table`
/// in the fast suite, while the `sync_pane_grids` revert leaves every headless test
/// green: a red smoke over a green fast suite is the signature of that deletion, and
/// is the most useful thing to know when this assertion fires.
#[test]
fn column_cap_and_full_width_toggle_change_the_grid_under_cage() {
    const SMOKE: &str = "column_cap_and_full_width_toggle_change_the_grid_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Mandatory: the swap directory is keyed on the cwd and outlives the process, so a leftover swap
    // raises a prompt that eats the next run's first keystroke.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let width_capped = runtime_dir.join(FULLWIDTH_CAPPED_READBACK_NAME);
    let width_full = runtime_dir.join(FULLWIDTH_FULL_READBACK_NAME);
    for path in [&width_capped, &width_full] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(FULLWIDTH_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("WIDTH_CAPPED", &width_capped)
        .env("WIDTH_FULL", &width_full);

    // At SMOKE_FONT_PT the cap cannot bind in a 989 px pane; this later `.env` wins and puts the app in
    // the one regime where it is observable.
    cmd.env("WOLF_FONT_PT", FULLWIDTH_SMOKE_FONT_PT);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless column-cap scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage column-cap scenario exited unsuccessfully ({status}). Exit 11: the app \
         died before it was ready for input — at {FULLWIDTH_SMOKE_FONT_PT} pt a panic in the cell \
         measurement or in `min_pane_size` is the first thing to look at. Exit 12: the CAPPED \
         readback never appeared at {}, so the app never answered at all. Exit 13: the FULL-WIDTH \
         readback never appeared at {} — it answered once and then stopped, which is what a panic \
         on the `:WolfFullWidth` drain path looks like from out here. Exit 14: still alive after \
         `:qa!`. Exit 15: it went away, but not cleanly.",
        width_capped.display(),
        width_full.display()
    );

    // Precondition: a missing or unparseable file means the app never answered, not that the cap is
    // wrong; the two need different fixes.
    let capped_raw = read_trimmed(&width_capped, SMOKE);
    let full_raw = read_trimmed(&width_full, SMOKE);
    let capped = parse_readback_int(&capped_raw, &width_capped, &capped_raw, SMOKE);
    let full = parse_readback_int(&full_raw, &width_full, &full_raw, SMOKE);
    for (label, cols) in [("capped", capped), ("full-width", full)] {
        assert!(
            cols > 0,
            "{SMOKE}: the {label} readback reported winwidth(0) = {cols}. A non-positive column \
             count means the pane never bound a grid, so the comparisons below would be between a \
             real measurement and a placeholder."
        );
    }

    // Assertion 2: compared against the production constant, not a retyped 90, so the two cannot drift.
    let expected_cap = i64::from(wolf_gnome::editor_inset::MAX_GRID_COLS);
    assert_eq!(
        capped, expected_cap,
        "{SMOKE}: with the content-width cap in force the captive Neovim reported \
         winwidth(0) = {capped}, want exactly {expected_cap} \
         (`editor_inset::MAX_GRID_COLS`). A count well ABOVE the cap — around 123 at \
         {FULLWIDTH_SMOKE_FONT_PT} pt in this pane — is the signature of a build that never \
         applies it: check that `editor_inset::grid_size_for_pane` still returns \
         `cap.apply(cols)` rather than `cols`, and that `pane_view` passes the pane's own cap \
         rather than an uncapped one. A count BELOW it means the pane is narrower than the cap \
         here, at which point this smoke is measuring nothing and the font size or the window \
         size has moved."
    );

    // --- Assertion 3: the toggle lifts it -------------------------------------
    assert!(
        full > capped,
        "{SMOKE}: after `:WolfFullWidth` the captive Neovim reported winwidth(0) = {full}, which \
         is not more than the {capped} it reported while capped (expect roughly 123 here). EQUAL \
         counts mean the toggle changed no geometry, and there are four candidate causes, in the \
         order the value travels: (1) `command_action::ui_action_for` still classifies \
         `Command::FullWidth` as `Deferred`, so the drained command never becomes an action at \
         all; (2) `shell::perform` has no `UiAction::ToggleFullWidth` arm, so the action never \
         becomes an `InputEvent`; (3) `PaneInner::on_render_wake` does not call \
         `sync_pane_grids`, so the flag flips and the paint re-clips but NO `resize_grid` is ever \
         issued — the widget allocation does not change when the cap lifts, so `connect_resize` \
         never fires and nothing else would tell Neovim; or (4) `editor_inset::column_cap` ignores \
         its flag and answers the default cap either way. A count LOWER than the capped one would \
         mean the toggle is wired backwards."
    );
}
